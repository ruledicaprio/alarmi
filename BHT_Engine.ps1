# BHT Alarm Engine v13.2 - Final Production Grade
$baseDir = "C:\Users\Rusmir\alarmi"
Set-Location $baseDir
$statsFile = "$baseDir\stats_data.json"
$historyFile = "$baseDir\history_data.json"

# Inicijalizacija baze za sedmicni report
if (-not (Test-Path $historyFile)) { "{}" | Set-Content $historyFile -Encoding UTF8 }

Write-Host ">>> BHT ENGINE v13.2 START <<<" -ForegroundColor Cyan

while ($true) {
    try {
        $now = Get-Date
        $todayKey = $now.ToString("yyyy-MM-dd")
        $allEvents = @()

        # FETCH: CSV (Ignition/NetEco) & TEXT (Network)
        $sources = @(
            @{ Uri = "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap"; Type = "CSV" },
            @{ Uri = "https://pokrivenost.bhtelecom.ba/alarmi/"; Type = "TEXT" }
        )

        foreach ($src in $sources) {
            try {
                $resp = Invoke-WebRequest -Uri $src.Uri -UseBasicParsing -TimeoutSec 15
                $lines = [System.Text.Encoding]::UTF8.GetString($resp.Content).Split("`n")
                
                if ($src.Type -eq "CSV") {
                    foreach ($line in $lines) {
                        if ($line -notlike "*,*") { continue }
                        $p = $line.Split(',').ForEach({ $_.Trim().Trim('"') })
                        $tsIdx = 0..($p.Count-1) | Where-Object { $p[$_] -match '\d{4}-\d{2}-\d{2}' } | Select-Object -First 1
                        if ($null -eq $tsIdx) { continue }

                        $sys = $p[0]; $rawSite = $p[1]; $alarm = $p[2]
                        $time = [DateTime]::Parse($p[$tsIdx].Replace('_',' '))
                        
                        # --- ROBUSNO PARSIRANJE (The Core Logic) ---
                        $region = "N/A"; $siteId = $rawSite.ToUpper()
                        
                        if ($sys -eq "IgnitionSCADA" -and $rawSite -match '^(.*?) - (.*)$') {
                            $region = $Matches[1].Trim()
                            $remainder = $Matches[2].Trim()
                            # Filtriranje tipova lokacija (RSU, ATC, etc.)
                            if ($remainder -match '(?:US/BS|US|BS|TKC|RSU|ATC|FTTB|HOST|MSAN)\s+(.*)$') {
                                $siteId = $Matches[1].Trim().ToUpper()
                            } else { $siteId = $remainder.ToUpper() }
                        }
                        
                        $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" }
                        if ($sys -eq "IgnitionSCADA") { $status = "UNKNOWN" } 

                        $allEvents += [PSCustomObject]@{ System=$sys; Region=$region; Site=$siteId; Alarm=$alarm; Time=$time; Status=$status }
                    }
                } else {
                    foreach ($line in $lines) {
                        if ($line -match '^(.*?)(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})(.*)$') {
                            $allEvents += [PSCustomObject]@{ System="NETWORK"; Region="N/A"; Site=$Matches[1].Trim().ToUpper(); Alarm="NE Disconnected"; Time=[DateTime]$Matches[2]; Status="ACTIVE" }
                        }
                    }
                }
            } catch { Write-Host "Izvor nedostupan: $($src.Uri)" -ForegroundColor Yellow }
        }

        # DNEVNA AGREGACIJA
        $daily = $allEvents | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $dur = 0; $cnt = 0
            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED" -and $g[$i].Time.Date -eq $now.Date) {
                    $dur += ($g[$i+1].Time - $g[$i].Time).TotalMinutes; $cnt++
                }
            }
            [PSCustomObject]@{ Site=$_.Values[0]; Alarm=$_.Values[1]; System=$g[0].System; Region=$g[0].Region; DayCnt=$cnt; DayDur=[Math]::Round($dur,1); LastStatus=$g[-1].Status }
        }

        # SEDMICNA PERSISTENCIJA (FIX ZA PROPERTY ERROR)
        $history = Get-Content $historyFile -Raw | ConvertFrom-Json
        if ($null -eq $history) { $history = New-Object PSObject }
        $history | Add-Member -NotePropertyName $todayKey -NotePropertyValue $daily -Force

        # Ciscenje (zadrzi 7 dana)
        $limit = (Get-Date).AddDays(-7).ToString("yyyy-MM-dd")
        $history.PSObject.Properties | Where-Object { $_.Name -lt $limit } | ForEach-Object { $history.PSObject.Properties.Remove($_.Name) }
        $history | ConvertTo-Json -Depth 10 | Set-Content $historyFile -Encoding UTF8

        # Agregacija za Weekly Tab
        $allHistoryData = $history.PSObject.Properties.Value | ForEach-Object { $_ }
        if ($null -eq $allHistoryData) { $allHistoryData = @() }

        $weekly = $allHistoryData | Group-Object Site, Alarm | ForEach-Object {
            [PSCustomObject]@{
                Site=$_.Values[0]; Alarm=$_.Values[1]; System=$_.Group[0].System; Region=$_.Group[0].Region;
                WeekCnt=($_.Group | Measure-Object DayCnt -Sum).Sum;
                WeekDur=[Math]::Round(($_.Group | Measure-Object DayDur -Sum).Sum, 1)
            }
        }

        # FINAL EXPORT
        $out = @{ LastUpdate=$now.ToString("yyyy-MM-dd HH:mm:ss"); Daily=$daily; Weekly=$weekly; Recent=$allEvents | Select-Object System, Site, Alarm, Time, Status -First 200 }
        $out | ConvertTo-Json -Depth 10 | Set-Content $statsFile -Encoding UTF8
        
        Write-Host "Sync OK @ $($now.ToString('HH:mm:ss'))" -ForegroundColor Green

    } catch { Write-Host "Fatal Error: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}