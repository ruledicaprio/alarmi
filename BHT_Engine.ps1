# BHT Alarm Engine v13.6 - Konsolidovana Verzija
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
} catch { Write-Host "Napomena: Baza regija nije učitana." -ForegroundColor Yellow }

if (-not (Test-Path $historyFile)) { "{}" | Set-Content $historyFile -Encoding UTF8 }

Write-Host ">>> BHT ENGINE v13.6 START <<<" -ForegroundColor Cyan

while ($true) {
    try {
        $now = Get-Date
        $todayKey = $now.ToString("yyyy-MM-dd")
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
                        
                        $region = "N/A"; $siteId = $rawSite.ToUpper()
                        if ($sys -eq "IgnitionSCADA" -and $rawSite -match '^(.*?) - (.*)$') {
                            $region = $Matches[1].Trim(); $siteId = $Matches[2].Trim().ToUpper()
                        }
                        if ($region -eq "N/A" -and $siteMap.ContainsKey($siteId)) { $region = $siteMap[$siteId] }
                        
                        $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" }
                        if ($sys -eq "IgnitionSCADA") { $status = "N/A" } 

                        $allEvents += [PSCustomObject]@{ System=$sys; Region=$region; Site=$siteId; Alarm=$alarm; Time=$timeStr; Status=$status }
                    }
                }
            } catch { }
        }

        $daily = $allEvents | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $dur = 0; $cnt = 0
            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED") {
                    $dur += ([DateTime]$g[$i+1].Time - [DateTime]$g[$i].Time).TotalMinutes; $cnt++
                }
            }
            [PSCustomObject]@{ 
                System=$g[0].System; Region=$g[0].Region; Site=$_.Values[0]; Alarm=$_.Values[1]; 
                LastTime=$g[-1].Time; DayCnt=$cnt; DayDur=[Math]::Round($dur,1); LastStatus=$g[-1].Status 
            }
        }

        $history = Get-Content $historyFile -Raw | ConvertFrom-Json
        if ($null -eq $history) { $history = New-Object PSObject }
        $history | Add-Member -NotePropertyName $todayKey -NotePropertyValue $daily -Force
        $history | ConvertTo-Json -Depth 10 | Set-Content $historyFile -Encoding UTF8

        $out = @{ LastUpdate=$now.ToString("yyyy-MM-dd HH:mm:ss"); Daily=$daily; Recent=$allEvents | Select-Object -First 200 }
        $out | ConvertTo-Json -Depth 10 | Set-Content $statsFile -Encoding UTF8
        Write-Host "Sync OK @ $($now.ToString('HH:mm:ss'))" -ForegroundColor Green

    } catch { Write-Host "Greška: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}