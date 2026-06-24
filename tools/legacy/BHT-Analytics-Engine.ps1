# BHT Alarm Engine v8 - PowerShell Edition
$url = "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap"
$baseDir = Get-Location
$masterLog = "$baseDir\master_alarms.log"
$statsFile = "$baseDir\stats_data.json"

Write-Host "🚀 BHT Engine pokrenut. Press Ctrl+C za zaustavljanje." -ForegroundColor Cyan

while ($true) {
    try {
        # 1. Fetch podataka (Ignorišemo SSL greške za svaki slučaj)
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $resp = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 30
        $raw = [System.Text.Encoding]::UTF8.GetString($resp.Content).Trim()
        
        if ($raw) {
            Add-Content -Path $masterLog -Value $raw -Encoding UTF8
        }

        # 2. Učitavanje i limitiranje na 100,000 linija
        $allLines = Get-Content $masterLog | Select-Object -Last 100000
        $events = @()
        $seen = New-Object System.Collections.Generic.HashSet[string]
        $now = Get-Date
        $today = $now.Date

        foreach ($line in $allLines) {
            if ($line -notlike "*,*") { continue }
            $p = $line.Split(',').ForEach({ $_.Trim().Trim('"') })
            
            # Detekcija sistema i polja
            $sys = $p[0]; $site = $p[1]; $alarm = $p[2]
            $tsIdx = 0..($p.Count-1) | Where-Object { $p[$_] -match '\d{4}-\d{2}-\d{2}' } | Select-Object -First 1
            if ($null -eq $tsIdx) { continue }
            
            $ts = [DateTime]::Parse($p[$tsIdx].Replace('_',' '))
            
            # Unikatni ključ za de-duplikaciju
            $key = "$sys|$site|$alarm|$($ts.ToString('yyyyMMddHHmmss'))"
            if (-not $seen.Add($key)) { continue }

            # FILTER LOGIKA: Izbaci dupli NetEco Mains ako imamo tehnološki alarm
            if ($sys -eq "NetEco" -and $alarm -eq "Mains Failure") { continue }

            $events += [PSCustomObject]@{
                System = $sys
                Site   = $site
                Alarm  = $alarm
                Time   = $ts
                IsToday = $ts -ge $today
                Status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" }
            }
        }

        # 3. Kalkulacija statistike (Danas, Sedmica)
        $grouped = $events | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $durDay = 0; $cntDay = 0
            $durWeek = 0; $cntWeek = 0

            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED") {
                    $diff = ($g[$i+1].Time - $g[$i].Time).TotalMinutes
                    if ($g[$i].IsToday) { $durDay += $diff; $cntDay++ }
                    if ($g[$i].Time -ge $now.AddDays(-7)) { $durWeek += $diff; $cntWeek++ }
                }
            }

            [PSCustomObject]@{
                System = $g[0].System
                Site   = $_.Values[0]
                Alarm  = $_.Values[1]
                DayCnt = $cntDay
                DayDur = [Math]::Round($durDay, 1)
                WeekCnt = $cntWeek
                WeekDur = [Math]::Round($durWeek, 1)
                TotalCnt = $g.Count
                LastStatus = $g[-1].Status
            }
        }

        # 4. Export u JSON za Dashboard
        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $grouped | Sort-Object DayCnt -Descending
            Recent = $events | Sort-Object Time -Descending | Select-Object -First 500 | ForEach-Object {
                $_.Time = $_.Time.ToString("yyyy-MM-dd HH:mm:ss"); $_
            }
        }
        
        $output | ConvertTo-Json -Depth 5 | Set-Content "$baseDir\stats_data.json" -Encoding UTF8
        Write-Host "[$(Get-Date -Format 'HH:mm:ss')] ✅ Podaci ažurirani." -ForegroundColor Green
    }
    catch {
        Write-Host "❌ Greška: $($_.Exception.Message)" -ForegroundColor Red
    }
    Start-Sleep -Seconds 60
}