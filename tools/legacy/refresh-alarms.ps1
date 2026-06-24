# BHTelecom Alarm Auto-Refresher (APPEND mode) – finalna verzija
Write-Host "🚀 BHTelecom Dashboard Engine pokrenut..." -ForegroundColor Green
Write-Host "Dodajem nove alarme svakih 60s... (Ctrl+C za stop)" -ForegroundColor Yellow

$targetPath = "E:\alarm-dashboard\alarms.txt"
$maxLines = 10000

if (-not (Test-Path "E:\alarm-dashboard")) {
    New-Item -Path "E:\alarm-dashboard" -ItemType Directory -Force | Out-Null
}

while ($true) {
    try {
        $url = "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap"
        $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 30 -UserAgent "BHTelecom-Alarm-Dashboard/1.0"

        $header = "`n=== NEW FETCH $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss') ===`n"
        
        Add-Content -Path $targetPath -Value $header -Encoding UTF8
        Add-Content -Path $targetPath -Value $response.Content -Encoding UTF8

        # Ograničenje veličine fajla (zadnjih 10.000 linija)
        $content = Get-Content $targetPath -Encoding UTF8
        if ($content.Count -gt $maxLines) {
            $content = $content[-$maxLines..-1]
            Set-Content -Path $targetPath -Value $content -Encoding UTF8
        }

        Write-Host "[$(Get-Date -Format 'HH:mm:ss')] ✅ Podaci ažurirani (Max $maxLines linija zadržano)" -ForegroundColor Green
    }
    catch {
        Write-Host "[$(Get-Date -Format 'HH:mm:ss')] ❌ Greška pri preuzimanju: $($_.Exception.Message)" -ForegroundColor Red
    }
    Start-Sleep -Seconds 60
}