# 🛠️ EA Denuvo Token Dumper

A lightweight **Windows tool** written in PowerShell and c++ (with chatgpt) that extracts the **Denuvo Token** from EA license files (`*.dlf`) and updates an `anadius.cfg` file automatically.  

This project is fully **open source** to reduce false positives and build trust — you can inspect the PowerShell code and build the executable yourself.

---

## ✨ Features
- 🔑 Extracts **Denuvo Token** from EA license files  
- 📝 **backs up & updates** an `anadius.cfg` file automatically  
- 💾 Inject to a process and automatic task - DLL version  
- 🖥️ Clean and simple **Windows Forms GUI**  
- 📋 Displays the extracted token in the app for easy copy-paste  

---

## 📥 Download
Grab the latest release here 👉 [Releases](../../releases)

You’ll find:
- `EA Denuvo Token Dumper.exe` – prebuilt Windows binary  
- `token_dumper.ps1` – PowerShell source code  
- `ShowcaseTokenDumper.zip` – prebuilt dll & dependecies

Each release includes a **SHA256 hash** so you can verify integrity.

---

## 🚀 Usage

### PowerShell → EXE
1. Launch the tool:  
   - Run **`EA Denuvo Token Dumper.exe`**, or  
   - Open **`token_dumper.ps1`** with PowerShell  

2. Select your **license file** (`*.dlf`).  
   - Default location:  `C:\ProgramData\Electronic Arts\EA Services\License\16425677_sc.dlf`

3. (Optional) Select your **config file** (`anadius.cfg`).  
   - ✅ “Add DenuvoToken to anadius.cfg even if it exists”  
   - If checked → token will be written into `anadius.cfg`  
   - If unchecked → token is only shown in the app  

4. Click **Start** to extract the token.  

---

### ⚡ Native C++ DLL
   - Especially made for FC26 Showcase version but could work with other title too (just need more tweaking)
   - Copy the file in the zip in release(or build your own) to game directory
   - Add this line to anadius.cfg in Emulator section:  
        **"LoadExtraDLLsFromMain" "ShowcaseTokenDumper.dll"**
   - Run the game

---


## 🔨 Build Instructions

### PowerShell → EXE
Want to build your own executable? Easy:

1. Install [ps2exe](https://www.powershellgallery.com/packages/ps2exe):
   ```powershell
   Install-Module -Name ps2exe -Scope CurrentUser -Force
   ```

2. Build with:
   ```powershell
   Invoke-ps2exe .\token_dumper.ps1 ".\EA Denuvo Token Dumper.exe" -noConsole
   ```

3. The resulting `EA Denuvo Token Dumper.exe` will be in your folder.

---

### ⚡ Native C++ DLL
For those who prefer a native implementation, a C++ DLL version is included under the `native/` folder.

#### Dependencies
Dependencies are managed with [vcpkg](https://github.com/microsoft/vcpkg):

```bash
vcpkg install tinyxml2:x64-windows cryptopp:x64-windows
```

#### Build with CMake
```bash
cd native
cmake -S . -B build -G "Visual Studio 17 2022" -A x64 ^
      -DCMAKE_TOOLCHAIN_FILE=C:/vcpkg/scripts/buildsystems/vcpkg.cmake ^
      -DVCPKG_TARGET_TRIPLET=x64-windows -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release
```

This will output:
```
native/build/Release/ShowcaseTokenDumper.dll
```

The DLL can be loaded with `rundll32` or injected into another process.

---

## 🛡️ Security & False Positives
This project is not malware.  
Antivirus tools sometimes flag self-built PowerShell → EXE or DLL files due to heuristics.

✅ Source code is public and auditable  
✅ SHA256 hashes provided for every release  
✅ You can rebuild the binary yourself from source  

If you encounter false positives, please open an issue.

---

## 🙏 Credits
- Developed by **RMC**  
- Special thanks to **Sodium.exe** and **anadius** (and ChatGPT too)  
- If you use or redistribute this project, please leave proper credits.

---

## License
This project is licensed under the **Community Use License (CUL-1.0)**.  
You are free to use and share this software with anyone, as long as the software is shared in its original, unmodified form and includes this copyright notice and license.  
Modifications are allowed only for private use or contributions submitted back to this repository.  
Forks, redistributions of modified versions, or removal of credits are prohibited.  
See the [LICENSE](LICENSE) file for full details.
