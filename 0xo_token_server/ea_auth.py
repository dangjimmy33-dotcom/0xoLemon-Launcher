import hashlib
import base64
import os
import httpx
import json
from typing import Optional

EA_AUTH_URL = "https://accounts.ea.com/connect/auth"
EA_TOKEN_URL = "https://accounts.ea.com/connect/token"
DENUVO_URL = "https://proxy.novafusion.ea.com/licenses"
CLIENT_ID = "JUNO_PC_CLIENT"

def generate_pkce():
    code_verifier = base64.urlsafe_b64encode(os.urandom(32)).decode('utf-8').rstrip('=')
    code_challenge = hashlib.sha256(code_verifier.encode('utf-8')).digest()
    code_challenge = base64.urlsafe_b64encode(code_challenge).decode('utf-8').rstrip('=')
    return code_verifier, code_challenge

async def get_access_token(config_path: str = "config.json") -> str:
    try:
        with open(config_path, "r", encoding="utf-8") as f:
            config = json.load(f)
            
        # Get refresh token
        accounts = config.get("accounts", [])
        if not accounts:
            raise Exception("Chưa cấu hình tài khoản trong config.json")
            
        refresh_token = accounts[0].get("refresh_token")
        if not refresh_token:
            raise Exception("Không tìm thấy refresh_token. Vui lòng chạy login_helper.py để đăng nhập lại.")

        token_data = {
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
            "client_secret": "4mRLtYMb6vq9qglomWEaT4ChxsXWcyqbQpuBNfMPOYOiDmYYQmjuaBsF2Zp0RyVeWkfqhE9TuGgAw7te",
            "token_format": "JWS"
        }
        
        async with httpx.AsyncClient() as client:
            res = await client.post("https://accounts.ea.com/connect/token", data=token_data)
            
            if res.status_code == 200:
                data = res.json()
                new_access_token = data.get("access_token")
                new_refresh_token = data.get("refresh_token")
                
                # Cập nhật refresh token mới vào config (vì nó bị thay đổi sau mỗi lần request)
                if new_refresh_token and new_refresh_token != refresh_token:
                    accounts[0]["refresh_token"] = new_refresh_token
                    with open(config_path, "w", encoding="utf-8") as f:
                        json.dump(config, f, indent=4)
                        
                return new_access_token
            else:
                raise Exception(f"Lỗi khi lấy access token: {res.text}")

    except Exception as e:
        raise e

async def generate_denuvo_token(access_token: str, machine_hash: str, ticket_data: str) -> Optional[str]:
    """
    Gửi ticket và access_token để xin Denuvo Token.
    Trích xuất token từ response .dlf (AES-CBC).
    """
    try:
        ticket_raw, engine, content_id = ticket_data.split('|')
    except ValueError:
        print("Invalid ticket format. Expected format: Base64Ticket|Engine|ContentID")
        return None

    params = {
        'contentId': content_id,
        'machineHash': machine_hash,
        'ea_eadmtoken': access_token,
        'requestToken': ticket_raw,
        'requestType': engine
    }

    headers = {
        'User-Agent': 'EACTransaction',
        'X-Requester-Id': 'Origin Online Activation'
    }
    
    async with httpx.AsyncClient() as client:
        res = await client.get("https://proxy.novafusion.ea.com/licenses", headers=headers, params=params)
        
        if res.status_code == 200 and res.headers.get('content-type', '').startswith('application/octet-stream'):
            return extract_dlf_token(res.content)
        else:
            print(f"Failed to generate denuvo token (HTTP {res.status_code}): {res.text}")
            return None

from Cryptodome.Cipher import AES
from Cryptodome.Util.Padding import unpad
import xml.etree.ElementTree as ET

def extract_dlf_token(data: bytes) -> str:
    key = bytes([65,50,114,45,208,130,239,176,220,100,87,197,118,104,202,9])
    iv = bytes(16)
    
    def try_decrypt(d):
        try:
            cipher = AES.new(key, AES.MODE_CBC, iv)
            decrypted = unpad(cipher.decrypt(d), AES.block_size)
            return decrypted.decode('utf-8').lstrip('\xfe\xff')
        except Exception:
            return None
            
    xml_str = try_decrypt(data)
    if not xml_str and len(data) > 0x41:
        xml_str = try_decrypt(data[0x41:])
        
    if xml_str:
        root = ET.fromstring(xml_str)
        ns = {'ns': 'http://ea.com/license'}
        token_node = root.find('.//ns:GameToken', ns)
        if token_node is not None:
            return token_node.text.strip().replace(' ', '').replace('\n', '').replace('\r', '')
    return None
