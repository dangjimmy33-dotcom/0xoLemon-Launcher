import json
import os
from fastapi import FastAPI, HTTPException, Request
from pydantic import BaseModel
from ea_auth import get_access_token, generate_denuvo_token

app = FastAPI(title="0xo Token Server")

class GenerateTokenRequest(BaseModel):
    ticket: str

@app.post("/api/denuvo/generate")
async def api_generate_token(req: GenerateTokenRequest):
    # Đọc Access Token và Machine Hash từ file token_data.txt
    # Người dùng (chủ server) sẽ dùng tool token_generator.exe 1 lần để tạo ra token_data.txt
    # và copy file này vào thư mục 0xo_token_server (nó sẽ có hạn 3 ngày)
    token_data_path = os.path.join(os.path.dirname(__file__), 'token_data.txt')
    
    if not os.path.exists(token_data_path):
        raise HTTPException(status_code=500, detail="Chưa có file token_data.txt trên server. Vui lòng chạy token_generator.exe để tạo và copy vào thư mục 0xo_token_server.")
        
    try:
        with open(token_data_path, 'r', encoding='utf-8') as f:
            token_data = json.load(f)
            
        access_token = token_data.get('last_access_token', [None, None])[1]
        machine_hash = token_data.get('machine_hash', '')
        
        if not access_token or not machine_hash:
            raise HTTPException(status_code=500, detail="Dữ liệu trong token_data.txt không hợp lệ.")
            
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Lỗi đọc token_data.txt: {str(e)}")

    # Gửi Ticket lên Denuvo để lấy Token bằng API chuẩn của EA
    denuvo_token = await generate_denuvo_token(access_token, machine_hash, req.ticket)
    if not denuvo_token:
        raise HTTPException(status_code=500, detail="Lỗi khi giải mã DLF hoặc lấy Denuvo Token từ EA API.")

    return {"token": denuvo_token}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=3030)
