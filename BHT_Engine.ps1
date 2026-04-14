# BHT Alarm Engine v13.7 - Final Structural Fix
$baseDir = "C:\Users\Rusmir\alarmi"
Set-Location $baseDir
$statsFile = "$baseDir\stats_data.json"
$historyFile = "$baseDir\history_data.json"

$siteMap = @{}
try {
    if (Test-Path "$baseDir\neteco_sites.csv") {
        Import-Csv "$baseDir\neteco_sites.csv" -Delimiter ";" | ForEach-Object { 
            $siteMap[$_.site_id.ToUpper()] = $_.region_id 
        }
    }
} catch { }

Write-Host ">>> BHT ENGINE v13.7 START <<<" -ForegroundColor Cyan

while ($true) {
    try {
        $now = Get-Date
        $allEvents = @()

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
                        $timeStr = $p[$tsIdx].Replace('_',' ')
                        $siteId = $rawSite.ToUpper()
                        $region = if ($siteMap.ContainsKey($siteId)) { $siteMap[$siteId] } else { "N/A" }
                        
                        $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" }
                        if ($sys -eq "IgnitionSCADA") { $status = "N/A" } 

                        $allEvents += [PSCustomObject]@{ System=$sys; Region=$region; Site=$siteId; Alarm=$alarm; Time=$timeStr; Status=$status }
                    }
                }
            } catch { }
        }

        # KLJUČNI FIX: Precizno mapiranje polja
        $daily = $allEvents | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $dur = 0; $cnt = 0
            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED") {
                    $dur += ([DateTime]$g[$i+1].Time - [DateTime]$g[$i].Time).TotalMinutes; $cnt++
                }
            }

            # Eksplicitno kreiranje polja koja JS očekuje
            [PSCustomObject]@{ 
                System     = [string]$g[0].System
                Region     = [string]$g[0].Region
                Site       = [string]$_.Values[0]
                Alarm      = [string]$_.Values[1]
                LastTime   = [string]$g[-1].Time
                DayCnt     = [int]$cnt
                DayDur     = [double][Math]::Round($dur,1)
                LastStatus = [string]$g[-1].Status
            }
        }

        $out = @{ 
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss"); 
            Daily = $daily; 
            Recent = $allEvents | Select-Object -First 100 
        }
        $out | ConvertTo-Json -Depth 10 | Set-Content $statsFile -Encoding UTF8
        Write-Host "Sync OK @ $($now.ToString('HH:mm:ss'))" -ForegroundColor Green

    } catch { Write-Host "Greška: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}