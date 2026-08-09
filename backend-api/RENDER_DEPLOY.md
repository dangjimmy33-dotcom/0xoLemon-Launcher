# Trien khai backend tren Render

Su dung Web Service hien co tai
`https://zeroxolemon-launcher.onrender.com`. Khong tao them service activation.

## Cau hinh service

- Root Directory: `backend-api`
- Build Command: `npm install`
- Start Command: `npm start`
- Health Check Path: `/health`

Tat ca bien trong [`.env.example`](.env.example) phai duoc cau hinh trong
Render Environment. Cac gia tri secret khong duoc commit, ghi vao log, hoac dua
vao React.

## Firestore

Backend dung cac collection rieng:

- `offlineActivationQuota`
- `offlineActivationAccounts`
- `offlineActivationRequests`
- `offlineActivationAudit`
- `offlineActivationSecrets`

Bat Firestore TTL cho field
`offlineActivationRequests.secretExpiresAt` de xoa token da ma hoa sau
cooldown. Quota va cooldown chi dung thoi gian UTC cua backend.

## Kiem tra sau deploy

```powershell
Invoke-RestMethod https://zeroxolemon-launcher.onrender.com/health
Invoke-RestMethod https://zeroxolemon-launcher.onrender.com/api/0xolemon/offline-activation/ea-sports-fc-26/status
```

Status phai tra `capacity: 5`, `serverTime`, `readiness` va metadata package
nhung khong duoc chua Discord ID, ticket, refresh token hay activation token.

Khong goi endpoint `activate` trong smoke test tu dong. Mot activation that chi
duoc chay khi nguoi dung chu dong xac nhan.
