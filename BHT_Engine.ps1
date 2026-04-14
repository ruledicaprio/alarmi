# BHT Alarm Engine v12 - Jednostavna i robusna verzija
$baseDir = "E:\BHT-Dashboard"
Set-Location $baseDir
$statsFile = "$baseDir\stats_data.json"

$previousCount = 0

while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $allEvents = @()

        # Izvor 1: CSV
        $raw1 = (Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap" -UseBasicParsing).Content
        $csvLines = [System.Text.Encoding]::UTF8.GetString($raw1).Split("`n")
        foreach ($line in $csvLines) {
            if ($line -notlike "*,*") { continue }
            $p = $line.Split(',').ForEach({ $_.Trim().Trim('"') })
            $tsIdx = 0..($p.Count-1) | Where-Object { $p[$_] -match '\d{4}-\d{2}-\d{2}' } | Select-Object -First 1
            if ($null -eq $tsIdx) { continue }
            
            $site = $p[1].ToUpper().Replace(" ", "_")
            $system = $p[0]
            if ($system -eq "IgnitionSCADA") {
                $status = "UNKNOWN"
            } else {
                $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" }
            }
            $allEvents += [PSCustomObject]@{
                System = $system; Site = $site; Alarm = $p[2]; 
                Time = [DateTime]::Parse($p[$tsIdx].Replace('_',' '));
                Status = $status
            }
        }

        # Izvor 2: Text table
        $raw2 = (Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing).Content
        $textLines = [System.Text.Encoding]::UTF8.GetString($raw2).Split("`n")
        $currentSection = "NETWORK"
        foreach ($line in $textLines) {
            if ($line -match '^---+\s*(.*?)\s*---+$') { $currentSection = $Matches[1].Trim(); continue }
            if ($line -match '^(.*?)(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})(.*)$') {
                $siteRaw = $Matches[1].Trim()
                $time = [DateTime]$Matches[2]
                $siteNorm = $siteRaw.Replace(" ", "_").ToUpper()
                $allEvents += [PSCustomObject]@{
                    System = $currentSection; Site = $siteNorm; Alarm = "NE Is Disconnected";
                    Time = $time; Status = "ACTIVE"
                }
            }
        }

        # Kalkulacija za sve osim Ignition
        $grouped = $allEvents | Where-Object { $_.System -ne "IgnitionSCADA" } | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $durDay = 0; $cntDay = 0
            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED" -and $g[$i].Time.Date -eq $today) {
                    $durDay += ($g[$i+1].Time - $g[$i].Time).TotalMinutes; $cntDay++
                }
            }
            [PSCustomObject]@{
                Site=$_.Values[0]; Alarm=$_.Values[1]; System=$g[0].System;
                DayCnt=$cntDay; DayDur=[Math]::Round($durDay, 1);
                Region="N/A"; LastStatus=$g[-1].Status
            }
        }

        # Ignition posebno
        $ignitionEvents = $allEvents | Where-Object { $_.System -eq "IgnitionSCADA" }
        $ignitionGrouped = $ignitionEvents | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group
            $cntDay = ($g | Where-Object { $_.Time.Date -eq $today }).Count
            [PSCustomObject]@{
                Site=$_.Values[0]; Alarm=$_.Values[1]; System="IgnitionSCADA";
                DayCnt=$cntDay; DayDur=0;
                Region="N/A"; LastStatus="UNKNOWN"
            }
        }

        $finalStats = $grouped + $ignitionGrouped

        # Ispis u konzolu
        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v12 - " $now.ToString("HH:mm:ss")
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih dogadjaja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 5 DNEVNIH ISPADADA:"
        $grouped | Where-Object { $_.DayDur -gt 0 } | Sort-Object DayDur -Descending | Select-Object -First 5 | ForEach-Object {
            Write-Host "  " $_.Site "-" $_.DayDur "min"
        }

        # JSON output
        $output = @{ 
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $finalStats
            Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500
        }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8
    }
    catch { Write-Host "GRESKA: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}