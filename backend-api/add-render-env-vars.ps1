# Script to add Firebase credentials to Render via API
# Requires Render API Key from: https://dashboard.render.com/u/settings#api-keys

param(
    [Parameter(Mandatory=$true)]
    [string]$RenderApiKey
)

$ErrorActionPreference = "Stop"

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Add Firebase Env Vars to Render" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Service ID from Render dashboard
$serviceId = "srv-ctmcj2u8ii6s73buikag"

# Read credentials files
Write-Host "[1/2] Reading Firebase credentials..." -ForegroundColor Yellow
$cred0xolemon = Get-Content "E:\007Launcher\backend-api\.env.0xolemon.txt" -Raw
$cred0xolemon1 = Get-Content "E:\007Launcher\backend-api\.env.0xolemon1.txt" -Raw

Write-Host "✅ Credentials loaded" -ForegroundColor Green
Write-Host ""

# Prepare API request headers
$headers = @{
    "Authorization" = "Bearer $RenderApiKey"
    "Content-Type" = "application/json"
}

$apiBase = "https://api.render.com/v1"

# Environment variables to add/update
$envVars = @(
    @{
        key = "FIREBASE_0XOLEMON_CREDENTIALS_JSON"
        value = $cred0xolemon.Trim()
    },
    @{
        key = "FIREBASE_0XOLEMON1_CREDENTIALS_JSON"
        value = $cred0xolemon1.Trim()
    }
)

Write-Host "[2/2] Adding environment variables to Render..." -ForegroundColor Yellow
Write-Host ""

foreach ($envVar in $envVars) {
    Write-Host "  Adding: $($envVar.key)" -ForegroundColor Cyan
    
    try {
        $body = @{
            key = $envVar.key
            value = $envVar.value
        } | ConvertTo-Json
        
        $response = Invoke-RestMethod `
            -Uri "$apiBase/services/$serviceId/env-vars" `
            -Method PUT `
            -Headers $headers `
            -Body $body
        
        Write-Host "  ✅ Added successfully" -ForegroundColor Green
    }
    catch {
        Write-Host "  ❌ Error: $($_.Exception.Message)" -ForegroundColor Red
        
        # Try to parse error response
        if ($_.ErrorDetails) {
            $errorJson = $_.ErrorDetails.Message | ConvertFrom-Json
            Write-Host "     Details: $($errorJson.message)" -ForegroundColor Red
        }
    }
    
    Write-Host ""
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "✅ DONE!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "1. Render will auto-deploy with new env vars" -ForegroundColor White
Write-Host "2. Check logs: https://dashboard.render.com/web/$serviceId/logs" -ForegroundColor White
Write-Host "3. Test: curl https://zeroxolemon-launcher.onrender.com/health" -ForegroundColor White
Write-Host ""
