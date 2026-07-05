// LumaCore - Steam client hook layer for SteaMidra.
// Copyright (c) 2025-2026 Midrag (https://github.com/Midrags).
// Distributed under the GNU General Public License v3 or later.
// See <https://www.gnu.org/licenses/> for the full license text.

#include "PatternSig.h"
#include "runtime/Logger.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <bcrypt.h>

#include <array>
#include <cstdint>
#include <cstring>
#include <vector>

#pragma comment(lib, "bcrypt.lib")

namespace PatternSig {

    namespace {

        // ── Embedded public key ──────────────────────────────────────────
        //
        // RSA 2048-bit public key in BCRYPT_RSAPUBLIC_BLOB format:
        //   BCRYPT_RSAKEY_BLOB header (Magic = BCRYPT_RSAPUBLIC_MAGIC,
        //                              BitLength = 2048,
        //                              cbPublicExp + cbModulus + 0 + 0)
        //   PublicExp (3 bytes for 0x010001 = 65537)
        //   Modulus  (256 bytes)
        //
        // PLACEHOLDER: when every byte of the modulus is 0, Verify()
        // returns KeyUnavailable. To wire signing in:
        //   1. openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out priv.pem
        //   2. openssl rsa -in priv.pem -pubout -RSAPublicKey_out -out pub.der -outform DER
        //   3. extract the 256-byte modulus from pub.der, paste into kPlaceholderModulus
        //   4. sign every shipped TOML with priv.pem using RSA-PSS-SHA256
        //      (salt length 32) and publish the hex-encoded signature at
        //      <toml_url>.sig alongside the TOML body
        //
        // The placeholder design keeps the build green during rollout.
        // Pre-key: every TOML reads as "KeyUnavailable" and the require_signed
        // setting decides whether to accept or reject. Post-key: real verify.
        constexpr std::size_t kModulusBytes = 256;
        constexpr std::array<std::uint8_t, 3> kPublicExp = { 0x01, 0x00, 0x01 };
        constexpr std::array<std::uint8_t, kModulusBytes> kPlaceholderModulus = {};

        bool IsKeyConfigured() {
            for (std::uint8_t b : kPlaceholderModulus) {
                if (b != 0) return true;
            }
            return false;
        }

        // Hex char to nibble. Returns -1 on non-hex.
        int HexNibble(char c) {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            return -1;
        }

        // Strip ASCII whitespace and parse the rest as hex bytes. Returns
        // false on any non-hex (or non-whitespace) character or when the
        // hex stream isn't a clean even number of nibbles.
        bool DecodeHex(std::string_view s, std::vector<std::uint8_t>& out) {
            out.clear();
            out.reserve(s.size() / 2);
            int high = -1;
            for (char c : s) {
                if (c == ' ' || c == '\t' || c == '\r' || c == '\n') continue;
                int n = HexNibble(c);
                if (n < 0) return false;
                if (high < 0) {
                    high = n;
                } else {
                    out.push_back(static_cast<std::uint8_t>((high << 4) | n));
                    high = -1;
                }
            }
            return high < 0;
        }

        bool ImportPublicKey(BCRYPT_ALG_HANDLE alg, BCRYPT_KEY_HANDLE& outKey) {
            outKey = nullptr;
            // Layout: BCRYPT_RSAKEY_BLOB | PublicExp | Modulus.
            std::vector<std::uint8_t> blob;
            blob.resize(sizeof(BCRYPT_RSAKEY_BLOB) + kPublicExp.size() + kModulusBytes);

            auto* hdr = reinterpret_cast<BCRYPT_RSAKEY_BLOB*>(blob.data());
            hdr->Magic       = BCRYPT_RSAPUBLIC_MAGIC;
            hdr->BitLength   = 2048;
            hdr->cbPublicExp = static_cast<ULONG>(kPublicExp.size());
            hdr->cbModulus   = static_cast<ULONG>(kModulusBytes);
            hdr->cbPrime1    = 0;
            hdr->cbPrime2    = 0;

            std::memcpy(blob.data() + sizeof(BCRYPT_RSAKEY_BLOB),
                        kPublicExp.data(), kPublicExp.size());
            std::memcpy(blob.data() + sizeof(BCRYPT_RSAKEY_BLOB) + kPublicExp.size(),
                        kPlaceholderModulus.data(), kModulusBytes);

            NTSTATUS s = BCryptImportKeyPair(alg, nullptr, BCRYPT_RSAPUBLIC_BLOB,
                                             &outKey, blob.data(),
                                             static_cast<ULONG>(blob.size()), 0);
            return s == 0;
        }

        bool Sha256(std::string_view body, std::array<std::uint8_t, 32>& digest) {
            BCRYPT_ALG_HANDLE  alg  = nullptr;
            BCRYPT_HASH_HANDLE hash = nullptr;
            bool ok = false;
            do {
                if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_SHA256_ALGORITHM,
                                                 nullptr, 0) != 0) break;
                if (BCryptCreateHash(alg, &hash, nullptr, 0, nullptr, 0, 0) != 0) break;
                if (BCryptHashData(hash,
                                    reinterpret_cast<PUCHAR>(const_cast<char*>(body.data())),
                                    static_cast<ULONG>(body.size()), 0) != 0) break;
                if (BCryptFinishHash(hash, digest.data(),
                                      static_cast<ULONG>(digest.size()), 0) != 0) break;
                ok = true;
            } while (false);
            if (hash) BCryptDestroyHash(hash);
            if (alg)  BCryptCloseAlgorithmProvider(alg, 0);
            return ok;
        }

    } // anonymous namespace

    Result Verify(std::string_view body, std::string_view sigBody) {
        if (sigBody.empty()) return Result::Missing;
        if (!IsKeyConfigured()) {
            // Placeholder modulus still in. Refuse to claim "Ok" because
            // we cannot actually verify anything. Caller falls back to
            // Settings::patternRequireSigned to decide what to do.
            return Result::KeyUnavailable;
        }

        std::vector<std::uint8_t> sigBytes;
        if (!DecodeHex(sigBody, sigBytes)) return Result::InvalidShape;
        if (sigBytes.size() != 256) return Result::InvalidShape;

        std::array<std::uint8_t, 32> digest{};
        if (!Sha256(body, digest)) return Result::SystemError;

        BCRYPT_ALG_HANDLE alg = nullptr;
        BCRYPT_KEY_HANDLE key = nullptr;
        Result outcome = Result::SystemError;
        do {
            if (BCryptOpenAlgorithmProvider(&alg, BCRYPT_RSA_ALGORITHM,
                                             nullptr, 0) != 0) break;
            if (!ImportPublicKey(alg, key)) break;

            BCRYPT_PSS_PADDING_INFO pss{};
            pss.pszAlgId = BCRYPT_SHA256_ALGORITHM;
            pss.cbSalt   = 32;

            NTSTATUS v = BCryptVerifySignature(key, &pss,
                                               digest.data(),
                                               static_cast<ULONG>(digest.size()),
                                               sigBytes.data(),
                                               static_cast<ULONG>(sigBytes.size()),
                                               BCRYPT_PAD_PSS);
            outcome = (v == 0) ? Result::Ok : Result::BadSignature;
        } while (false);

        if (key) BCryptDestroyKey(key);
        if (alg) BCryptCloseAlgorithmProvider(alg, 0);
        return outcome;
    }

    const char* ResultToStr(Result r) {
        switch (r) {
            case Result::Ok:              return "ok";
            case Result::Missing:         return "missing";
            case Result::InvalidShape:    return "invalid-shape";
            case Result::BadSignature:    return "bad-signature";
            case Result::KeyUnavailable:  return "key-unavailable";
            case Result::SystemError:     return "system-error";
        }
        return "?";
    }
}
