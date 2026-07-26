# Quick test script for backend

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Testing Backend Locally" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check if node_modules exists
if (-Not (Test-Path "node_modules")) {
    Write-Host "📦 Installing dependencies..." -ForegroundColor Yellow
    npm install
    Write-Host ""
}

# Check .env
if (-Not (Test-Path ".env")) {
    Write-Host "⚠️  .env not found, creating from example..." -ForegroundColor Yellow
    Copy-Item ".env.example" ".env"
    Write-Host "✅ Created .env file" -ForegroundColor Green
    Write-Host "📝 Edit .env if needed (already has default config)" -ForegroundColor Yellow
    Write-Host ""
}

Write-Host "🚀 Starting backend..." -ForegroundColor Cyan
Write-Host "   Press Ctrl+C to stop" -ForegroundColor Yellow
Write-Host ""

# Start server
npm start
