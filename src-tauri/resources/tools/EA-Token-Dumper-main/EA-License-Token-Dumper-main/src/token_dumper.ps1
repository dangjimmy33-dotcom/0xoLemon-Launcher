<#
    EA Denuvo Token Dumper
    Copyright (C) 2025 RMC
    Licensed under the Community Use License (CUL-1.0)
    See: [https://github.com/RMC-4/EA-License-Token-Dumper/blob/main/LICENSE](https://github.com/RMC-4/EA-License-Token-Dumper/blob/main/LICENSE)
#>

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# Colors
$colorFormBack = [System.Drawing.Color]::FromArgb(245,245,245)
$colorLabelFore = [System.Drawing.Color]::FromArgb(0,70,130)
$colorSignatureFore = [System.Drawing.Color]::FromArgb(100,100,100)

# Determine script directory and log file path
$scriptDir = if ($PSScriptRoot) {
    $PSScriptRoot
} elseif ([System.Reflection.Assembly]::GetEntryAssembly()) {
    Split-Path -Parent ([System.Reflection.Assembly]::GetEntryAssembly().Location)
} else {
    Get-Location
}
$logFilePath = Join-Path $scriptDir "dumper_log.txt"

# State to track if first log write happened (for overwrite vs append)
$global:logStarted = $false

# Logging function that overwrites on first call and appends after
function Write-Log {
    param([string]$message)
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $line = "[$timestamp] $message"

    if (-not $global:logStarted) {
        # Overwrite log file on first write
        Set-Content -Path $logFilePath -Value $line
        $global:logStarted = $true
    }
    else {
        # Append log on subsequent writes
        Add-Content -Path $logFilePath -Value $line
    }
}

# Log startup
Write-Log "Application started."

# --- ContentId detection ---
$anadiusCfgPath = Join-Path $scriptDir "anadius.cfg"
$actualContentId = $null
if (Test-Path $anadiusCfgPath) {
    try {
        $cfgText = Get-Content -Raw -LiteralPath $anadiusCfgPath
        if ($cfgText -match '"ContentId"\s*"\s*([0-9]+)\s*"') {
            $actualContentId = $Matches[1]
            Write-Log "Detected ContentId: $actualContentId"
        } else {
            Write-Log "ContentId not found in config."
        }
    }
    catch {
        Write-Log "Failed to read or parse config file: $_"
    }
} else {
    Write-Log "Config file not found at default path: $anadiusCfgPath"
}

# Main form setup
$form = New-Object System.Windows.Forms.Form
$form.Text = "EA Denuvo Token Dumper"
$form.Size = New-Object System.Drawing.Size(700,610)
$form.StartPosition = "CenterScreen"
$form.FormBorderStyle = 'FixedDialog'
$form.MaximizeBox = $false
$form.BackColor = $colorFormBack
$form.TopMost = $true

# License file label and textbox
$lblDLF = New-Object System.Windows.Forms.Label
$lblDLF.Text = "License file:"
$lblDLF.Font = New-Object System.Drawing.Font("Segoe UI",11,[System.Drawing.FontStyle]::Bold)
$lblDLF.ForeColor = $colorLabelFore
$lblDLF.Location = "20,15"
$form.Controls.Add($lblDLF)

$txtDLF = New-Object System.Windows.Forms.TextBox
$txtDLF.Size = "530,28"
$txtDLF.Location = "20,40"
$txtDLF.Font = New-Object System.Drawing.Font("Segoe UI",10)
$form.Controls.Add($txtDLF)

$btnDLF = New-Object System.Windows.Forms.Button
$btnDLF.Text = "Browse"
$btnDLF.Location = "565,39"
$btnDLF.Size = "90,30"
$btnDLF.Font = New-Object System.Drawing.Font("Segoe UI",9)
$btnDLF.Add_Click({
    $ofd = New-Object System.Windows.Forms.OpenFileDialog
    $ofd.Title = "Select License File"
    $ofd.Filter = "DLF files (*.dlf)|*.dlf|All files (*.*)|*.*"
    $ofd.InitialDirectory = Split-Path $txtDLF.Text -Parent
    if ($ofd.ShowDialog() -eq "OK") {
        $txtDLF.Text = $ofd.FileName
        Write-Log "User selected license file: $($txtDLF.Text)"
    }
})
$form.Controls.Add($btnDLF)

# Checkbox for FC26 showcase version
$chkShowcase = New-Object System.Windows.Forms.CheckBox
$chkShowcase.Text = "FC26 showcase version"
$chkShowcase.Location = "20,70"
$chkShowcase.Font = New-Object System.Drawing.Font("Segoe UI",10)
$chkShowcase.Width = 200
$chkShowcase.Checked = $true
$form.Controls.Add($chkShowcase)

# ContentId label next to checkbox
$lblContentId = New-Object System.Windows.Forms.Label
if ($actualContentId) {
    $lblContentId.Text = "Detected ContentId in config: $actualContentId"
} else {
    $lblContentId.Text = "No ContentId detected in config."
}
$lblContentId.Font = New-Object System.Drawing.Font("Segoe UI",10)
$lblContentId.ForeColor = $colorLabelFore
$lblContentId.Location = "230,70"
$lblContentId.Size = "400,24"
$form.Controls.Add($lblContentId)

function UpdateLicensePath {
    if ($chkShowcase.Checked) {
        $id = if ($actualContentId) { $actualContentId } else { "16425677" }
        $txtDLF.Text = "C:\ProgramData\Electronic Arts\EA Services\License\${id}_sc.dlf"
        $txtDLF.Enabled = $false
        $btnDLF.Enabled = $false
        $txtDLF.ForeColor = [System.Drawing.Color]::Gray
        Write-Log "Showcase version enabled, license path set to: $($txtDLF.Text)"
    } else {
        if ($actualContentId) {
            $txtDLF.Text = "C:\ProgramData\Electronic Arts\EA Services\License\$actualContentId.dlf"
        } else {
            $txtDLF.Text = "C:\ProgramData\Electronic Arts\EA Services\License\16425677_sc.dlf"
        }
        $txtDLF.Enabled = $true
        $btnDLF.Enabled = $true
        $txtDLF.ForeColor = [System.Drawing.Color]::Black
        Write-Log "Showcase version disabled, license path set to: $($txtDLF.Text)"
    }
}

$chkShowcase.Add_CheckedChanged({
    UpdateLicensePath
})

UpdateLicensePath

# Other controls...

$lblCFG = New-Object System.Windows.Forms.Label
$lblCFG.Text = "Config file:"
$lblCFG.Font = New-Object System.Drawing.Font("Segoe UI",11,[System.Drawing.FontStyle]::Bold)
$lblCFG.ForeColor = $colorLabelFore
$lblCFG.Location = "20,100"
$form.Controls.Add($lblCFG)

$txtCFG = New-Object System.Windows.Forms.TextBox
$txtCFG.Size = "530,28"
$txtCFG.Location = "20,125"
$txtCFG.Font = New-Object System.Drawing.Font("Segoe UI",10)
$txtCFG.Text = $anadiusCfgPath
$form.Controls.Add($txtCFG)

$btnCFG = New-Object System.Windows.Forms.Button
$btnCFG.Text = "Browse"
$btnCFG.Location = "565,124"
$btnCFG.Size = "90,30"
$btnCFG.Font = New-Object System.Drawing.Font("Segoe UI",9)
$btnCFG.Add_Click({
    $ofd = New-Object System.Windows.Forms.OpenFileDialog
    $ofd.Title = "Select Config File"
    $ofd.Filter = "CFG files (*.cfg)|*.cfg|All files (*.*)|*.*"
    $ofd.InitialDirectory = Split-Path $txtCFG.Text -Parent
    if ($ofd.ShowDialog() -eq "OK") {
        $txtCFG.Text = $ofd.FileName
        Write-Log "User selected config file: $($txtCFG.Text)"
    }
})
$form.Controls.Add($btnCFG)

$chkOverwrite = New-Object System.Windows.Forms.CheckBox
$chkOverwrite.Text = "Add DenuvoToken to anadius.cfg even if it exists"
$chkOverwrite.Location = "20,165"
$chkOverwrite.Font = New-Object System.Drawing.Font("Segoe UI",10)
$chkOverwrite.Width = 450
$chkOverwrite.Checked = $true
$form.Controls.Add($chkOverwrite)

$chkOverwrite.Add_CheckedChanged({
    $txtCFG.Enabled = $chkOverwrite.Checked
    $btnCFG.Enabled = $chkOverwrite.Checked
    Write-Log "Overwrite checkbox changed: $($chkOverwrite.Checked)"
})

$txtCFG.Enabled = $chkOverwrite.Checked
$btnCFG.Enabled = $chkOverwrite.Checked

$btnStart = New-Object System.Windows.Forms.Button
$btnStart.Text = "Start"
$btnStart.Font = New-Object System.Drawing.Font("Segoe UI",12,[System.Drawing.FontStyle]::Bold)
$btnStart.Location = "565,205"
$btnStart.Size = "90,40"
$form.Controls.Add($btnStart)

$lblTokenTitle = New-Object System.Windows.Forms.Label
$lblTokenTitle.Text = "DenuvoToken:"
$lblTokenTitle.Font = New-Object System.Drawing.Font("Segoe UI", 11, [System.Drawing.FontStyle]::Bold)
$lblTokenTitle.ForeColor = $colorLabelFore
$lblTokenTitle.Location = "20,260"
$lblTokenTitle.Size = "640,25"
$form.Controls.Add($lblTokenTitle)

$txtToken = New-Object System.Windows.Forms.TextBox
$txtToken.Location = "20,290"
$txtToken.Size = "635,180"
$txtToken.Font = New-Object System.Drawing.Font("Consolas",10)
$txtToken.Multiline = $true
$txtToken.ScrollBars = "Vertical"
$txtToken.ReadOnly = $true
$form.Controls.Add($txtToken)

$pbar = New-Object System.Windows.Forms.ProgressBar
$pbar.Style = "Continuous"
$pbar.Location = "20,490"
$pbar.Size = "635,25"
$form.Controls.Add($pbar)

$lblSignature = New-Object System.Windows.Forms.LinkLabel
$lblSignature.Text = "Made by RMC`nThanks to Sodium and anadius`nApp GitHub Repo: https://github.com/RMC-4/EA-License-Token-Dumper/"
$startIndex = $lblSignature.Text.IndexOf("https://github.com/RMC-4/EA-License-Token-Dumper/")
$null = $lblSignature.Links.Add($startIndex, ($lblSignature.Text.Length - $startIndex), "https://github.com/RMC-4/EA-License-Token-Dumper/")
$lblSignature.Font = New-Object System.Drawing.Font("Segoe UI", 8, [System.Drawing.FontStyle]::Italic)
$lblSignature.LinkColor = [System.Drawing.Color]::FromArgb(0, 70, 130)
$lblSignature.ActiveLinkColor = [System.Drawing.Color]::FromArgb(0, 70, 130)
$lblSignature.VisitedLinkColor = [System.Drawing.Color]::FromArgb(0, 70, 130)
$lblSignature.AutoSize = $true
$lblSignature.TextAlign = 'MiddleCenter'
$lblSignature.add_LinkClicked({
    Start-Process $lblSignature.Links[0].LinkData
    Write-Log "User clicked GitHub link"
})
$form.Controls.Add($lblSignature)

$form.Add_Shown({
    $x = [int](($form.ClientSize.Width - $lblSignature.PreferredWidth) / 2)
    $y = $pbar.Location.Y + $pbar.Height + 5
    $lblSignature.Location = New-Object System.Drawing.Point($x, $y)
})

function Show-Message {
    param(
        [string]$text,
        [string]$title,
        [string]$icon = 'None',
        [switch]$AlsoToToken
    )
    Write-Log "MessageBox shown - Title: $title, Text: $text"
    [System.Windows.Forms.MessageBox]::Show(
        $text, $title,
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::$icon
    ) | Out-Null

    if ($AlsoToToken) { $txtToken.Text = $text }
}

function Sleep-And-Update([int]$ms) {
    Start-Sleep -Milliseconds $ms
    [System.Windows.Forms.Application]::DoEvents()
}

function Try-Decrypt {
    param($inputBytes, $keyBytes, $iv)
    try {
        $aes = [System.Security.Cryptography.Aes]::Create()
        $aes.Mode = [System.Security.Cryptography.CipherMode]::CBC
        $aes.Padding = [System.Security.Cryptography.PaddingMode]::PKCS7
        $aes.Key = $keyBytes
        $aes.IV = $iv
        $decryptor = $aes.CreateDecryptor()
        $decrypted = $decryptor.TransformFinalBlock($inputBytes, 0, $inputBytes.Length)
        Write-Log "Decryption succeeded"
        return [System.Text.Encoding]::UTF8.GetString($decrypted)
    } catch {
        Write-Log "Decryption failed: $_"
        return $null
    }
}

$btnStart.Add_Click({
    Write-Log "Start button clicked"
    $Error.Clear()
    $pbar.Value = 10
    $txtToken.Text = "Starting process..."
    Sleep-And-Update 300

    $dlfPath = $txtDLF.Text
    $cfgPath = $txtCFG.Text

    if ([string]::IsNullOrEmpty($dlfPath) -or -not (Test-Path $dlfPath)) {
        Show-Message "License file not found or path empty:`n$dlfPath" "Error" 'Error' -AlsoToToken
        Write-Log "License file not found or path empty: $dlfPath"
        $pbar.Value = 0
        return
    }
    $txtToken.Text = "License file found. Reading..."
    Write-Log "License file found: $dlfPath"
    Sleep-And-Update 300

    if ($chkOverwrite.Checked) {
        if ([string]::IsNullOrWhiteSpace($cfgPath) -or -not (Test-Path $cfgPath)) {
            Show-Message "Config file not found or path empty:`n$cfgPath" "Error" 'Error' -AlsoToToken
            Write-Log "Config file not found or path empty: $cfgPath"
            $pbar.Value = 0
            return
        }
    }
    $txtToken.Text = "Reading license file..."
    Write-Log "Reading license file: $dlfPath"
    Sleep-And-Update 300

    $keyBytes = [byte[]](65,50,114,45,208,130,239,176,220,100,87,197,118,104,202,9)
    $iv = New-Object byte[] 16
    $bytes = [System.IO.File]::ReadAllBytes($dlfPath)
    $pbar.Value = 25
    $txtToken.Text = "Decrypting license file..."
    Sleep-And-Update 300

    $decryptedText = Try-Decrypt $bytes $keyBytes $iv

    if (-not $decryptedText) {
        if ($bytes.Length -le 0x41) {
            Show-Message "File too small for fallback decryption." "Error" "Error" -AlsoToToken
            Write-Log "File too small for fallback decryption"
            $pbar.Value = 0
            return
        }
        $sliceLength = $bytes.Length - 0x41
        $bytes2 = New-Object byte[] $sliceLength
        [Array]::Copy($bytes, 0x41, $bytes2, 0, $sliceLength)
        $txtToken.Text = "Trying fallback decryption..."
        Write-Log "Trying fallback decryption"
        Sleep-And-Update 300
        $decryptedText = Try-Decrypt $bytes2 $keyBytes $iv
    }

    $pbar.Value = 41
    if (-not $decryptedText) {
        Show-Message "Decryption failed. Invalid/corrupt license file." "Error" "Error" -AlsoToToken
        Write-Log "Decryption failed after fallback"
        $pbar.Value = 0
        return
    }
    $txtToken.Text = "Parsing decrypted XML..."
    Write-Log "Parsing decrypted XML"
    Sleep-And-Update 300

    try { [xml]$xml = $decryptedText.Trim([char]0xFEFF) }
    catch {
        Show-Message "Decrypted data is not valid XML." "Error" "Error" -AlsoToToken
        Write-Log "Decrypted data is not valid XML: $_"
        $pbar.Value = 0
        return
    }
    $pbar.Value = 59

    $nsManager = New-Object System.Xml.XmlNamespaceManager($xml.NameTable)
    $nsManager.AddNamespace("ns","http://ea.com/license")
    $tokenNode = $xml.SelectSingleNode("//ns:GameToken", $nsManager)
    if (-not $tokenNode) {
        Show-Message "GameToken not found in decrypted license." "Error" "Error" -AlsoToToken
        Write-Log "GameToken not found in decrypted license"
        $pbar.Value = 0
        return
    }
    $txtToken.Text = "Extracting GameToken..."
    Write-Log "Extracting GameToken"
    Sleep-And-Update 300

    $token = $tokenNode.InnerText -replace "\s+", ""
    $txtToken.Text = "Token extracted. Displaying..."
    Write-Log "Token extracted: $token"
    Sleep-And-Update 300
    $pbar.Value = 75
    $txtToken.Text = $token

    if ($chkOverwrite.Checked) {
        $txtToken.Text += "`r`n`r`nBacking up config and updating..."
        Write-Log "Backing up config and updating"
        Sleep-And-Update 300

        # Backup config
        $backup = "$cfgPath.bak"
        Copy-Item -Path $cfgPath -Destination $backup -Force
        Write-Log "Backup created: $backup"

        # Update config logic
        $configText = Get-Content -Raw -LiteralPath $cfgPath
        $pattern = '("DenuvoToken"\s*"\s*)(.*?)(\s*")'
        if ($configText -match $pattern) {
            # Replace existing token line only
            $updatedText = [regex]::Replace($configText, $pattern,
                { param($m) $m.Groups[1].Value + $token + $m.Groups[3].Value },
                [System.Text.RegularExpressions.RegexOptions]::Singleline)
            Write-Log "Updated existing DenuvoToken line in config"
        }
        else {
            # No existing token line, add one at the end
            if ([string]::IsNullOrWhiteSpace($configText) -or $configText[-1] -ne "`n") {
                $configText += "`n"
            }
            $updatedText = $configText + "`"DenuvoToken`" `"$token`"`n"
            Write-Log "Appended new DenuvoToken line in config"
        }

        Set-Content -LiteralPath $cfgPath -Value $updatedText -Encoding utf8
        Write-Log "Config file saved"

        $txtToken.Text += "`r`nUpdate completed successfully."
        Show-Message "DenuvoToken updated!`nBackup created at:`n$backup" "Success" "Information"
        Write-Log "DenuvoToken update succeeded"
    }
    else {
        Show-Message "DenuvoToken extracted successfully!" "Success" "Information"
        Write-Log "Extraction succeeded, no config overwrite"
    }

    $pbar.Value = 100
})

$form.Add_Shown({ $form.Activate() })
[System.Windows.Forms.Application]::EnableVisualStyles()
[System.Windows.Forms.Application]::Run($form)
