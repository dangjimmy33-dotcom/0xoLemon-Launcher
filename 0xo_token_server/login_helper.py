import os
import json
import base64
import hashlib
import urllib.parse
import httpx
import webbrowser

CLIENT_ID = "JUNO_PC_CLIENT"
REDIRECT_URI = "qrc:///html/login_successful.html"

def generate_pkce():
    code_verifier = base64.urlsafe_b64encode(os.urandom(32)).decode('utf-8').rstrip('=')
    code_challenge = hashlib.sha256(code_verifier.encode('utf-8')).digest()
    code_challenge = base64.urlsafe_b64encode(code_challenge).decode('utf-8').rstrip('=')
    return code_verifier, code_challenge

def get_tokens(code, code_verifier):
    token_data = {
        "grant_type": "authorization_code",
        "code": code,
        "client_id": CLIENT_ID,
        "client_secret": "4mRLtYMb6vq9qglomWEaT4ChxsXWcyqbQpuBNfMPOYOiDmYYQmjuaBsF2Zp0RyVeWkfqhE9TuGgAw7te",
        "code_verifier": code_verifier,
        "token_format": "JWS",
        "redirect_uri": REDIRECT_URI
    }
    res = httpx.post("https://accounts.ea.com/connect/token", data=token_data)
    if res.status_code == 200:
        return res.json()
    else:
        print("Lỗi khi lấy token:", res.text)
        return None

def main():
    verifier, challenge = generate_pkce()
    
    url = f"https://accounts.ea.com/connect/auth?client_id={CLIENT_ID}&response_type=code&redirect_uri={urllib.parse.quote(REDIRECT_URI)}&display=junoClient/login&code_challenge={challenge}&code_challenge_method=S256"
    
    print("="*60)
    print("Mở đường link sau trên trình duyệt và đăng nhập tài khoản EA:")
    print(url)
    print("="*60)
    
    webbrowser.open(url)
    
    print("\nSau khi đăng nhập xong, trình duyệt sẽ báo lỗi (Trang không hoạt động/qrc://).")
    print("ĐỪNG LO! Hãy nhìn lên THANH ĐỊA CHỈ (URL) của trình duyệt, bạn sẽ thấy nó có dạng:")
    print("qrc:///html/login_successful.html?code=XXXXXX")
    print("\nHãy copy cái chữ XXXXXX (sau dấu code=) và dán vào đây:")
    
    code = input("Nhập code: ").strip()
    
    if "code=" in code:
        code = code.split("code=")[1].split("&")[0]
        
    print("\nĐang xử lý...")
    tokens = get_tokens(code, verifier)
    
    if tokens:
        print("ĐĂNG NHẬP THÀNH CÔNG!")
        config_path = os.path.join(os.path.dirname(__file__), 'config.json')
        
        # Save refresh_token to config
        config_data = {"accounts": [{"id": "default", "refresh_token": tokens.get("refresh_token")}]}
        with open(config_path, 'w', encoding='utf-8') as f:
            json.dump(config_data, f, indent=4)
            
        print(f"Đã lưu refresh_token vào {config_path}")
        print("Bây giờ bạn có thể mở 0xoLauncher và ấn Active Denuvo!")
        
if __name__ == "__main__":
    main()
