# BHT Alarm Engine v22 - fixed detached HEAD + persistent counter
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta već radi." -ForegroundColor Red; exit }

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio"
git config user.email "rusmirskopljak@gmail.com"

# Učitaj prethodni broj događaja (ako postoji)
$previousCount = 0
if (Test-Path $counterFile) { $previousCount = [int](Get-Content $counterFile) }

while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $allEvents = @()
        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..."

        # --- CSV izvor ---
        try {
            $csvResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap" -UseBasicParsing -TimeoutSec 30
            $csvContent = [System.Text.Encoding]::UTF8.GetString($csvResponse.Content)
            $csvLines = $csvContent -split "`r?`n"
            foreach ($line in $csvLines) {
                if ([string]::IsNullOrWhiteSpace($line)) { continue }
                if ($line -notlike "*,*") { continue }
                $p = $line.Split(',').ForEach({ $_.Trim().Trim('"') })
                if ($p.Count -lt 3) { continue }
                $tsIdx = -1
                for ($i=0; $i -lt $p.Count; $i++) { if ($p[$i] -match '\d{4}-\d{2}-\d{2}[ _]\d{2}:\d{2}:\d{2}') { $tsIdx=$i; break } }
                if ($tsIdx -eq -1) { continue }
                $site = $p[1].Trim().ToUpper().Replace(" ", "_")
                $system = $p[0].Trim()
                $alarm = $p[2].Trim()
                $timeStr = $p[$tsIdx] -replace '_', ' '
                $time = [DateTime]::Parse($timeStr)
                if ($system -eq "IgnitionSCADA") { $status = "UNKNOWN" }
                else { $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" } }
                $allEvents += [PSCustomObject]@{ System=$system; Site=$site; Alarm=$alarm; Time=$time; Status=$status }
            }
        } catch { Write-Host "  CSV greška: $($_.Exception.Message)" -ForegroundColor Red }

        # --- HTML izvor ---
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = [System.Text.Encoding]::UTF8.GetString($htmlResponse.Content)
            $lines = $htmlContent -split "`r?`n"
            $currentSection = "NETWORK"
            foreach ($line in $lines) {
                if ($line -match '---+\s*([A-Z]+)\s*---+') { $currentSection = $Matches[1].Trim(); continue }
                if ($line -match '(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})') {
                    $timeStr = $Matches[1]
                    $time = [DateTime]::Parse($timeStr)
                    $beforeDate = $line.Substring(0, $line.IndexOf($timeStr)).Trim()
                    $siteRaw = $beforeDate -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' ' -replace '^\s+|\s+$', ''
                    if (-not [string]::IsNullOrWhiteSpace($siteRaw)) {
                        $siteNorm = $siteRaw.ToUpper().Replace(" ", "_")
                        $allEvents += [PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE Is Disconnected"; Time=$time; Status="ACTIVE" }
                    }
                }
            }
        } catch { Write-Host "  HTML greška: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host "  Ukupno događaja: $($allEvents.Count)"

        # --- Grupisanje ---
        $grouped = $allEvents | Where-Object { $_.System -ne "IgnitionSCADA" } | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $durDay=0; $cntDay=0
            for($i=0; $i -lt ($g.Count-1); $i++) {
                if ($g[$i].Status -eq "ACTIVE" -and $g[$i+1].Status -eq "CLEARED" -and $g[$i].Time.Date -eq $today) {
                    $durDay += ($g[$i+1].Time - $g[$i].Time).TotalMinutes; $cntDay++
                }
            }
            [PSCustomObject]@{
                Site = $_.Values[0]; Alarm = $_.Values[1]; System = $g[0].System;
                DayCnt = $cntDay; DayDur = [Math]::Round($durDay,1);
                Region = "N/A"; LastStatus = $g[-1].Status
            }
        }
        $ignitionEvents = $allEvents | Where-Object { $_.System -eq "IgnitionSCADA" }
        $ignitionGrouped = $ignitionEvents | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group
            $cntDay = ($g | Where-Object { $_.Time.Date -eq $today }).Count
            [PSCustomObject]@{
                Site = $_.Values[0]; Alarm = $_.Values[1]; System = "IgnitionSCADA";
                DayCnt = $cntDay; DayDur = 0; Region = "N/A"; LastStatus = "UNKNOWN"
            }
        }
        $finalStats = $grouped + $ignitionGrouped

        # --- Ispis sa perzistentnim brojačem ---
        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force   # sačuvaj za sljedeći ciklus

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v22 - $($now.ToString('HH:mm:ss'))"
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih događaja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 5 DNEVNIH ISPADADA:"
        $grouped | Where-Object { $_.DayDur -gt 0 } | Sort-Object DayDur -Descending | Select-Object -First 5 | ForEach-Object {
            Write-Host "  $($_.Site) - $($_.DayDur) min"
        }

        # --- JSON izlaz ---
        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $finalStats
            Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500
        }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        # --- Git: osiguraj da smo na grani main ---
        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        $currentBranch = git rev-parse --abbrev-ref HEAD 2>$null
        if ($currentBranch -ne "main") {
            Write-Host "Nismo na grani main, vraćam se..." -ForegroundColor Yellow
            git checkout main 2>&1 | Out-Null
            if ($LASTEXITCODE -ne 0) {
                # Ako main ne postoji lokalno, kreiraj je
                git checkout -b main 2>&1 | Out-Null
                git branch --set-upstream-to=origin/main main 2>&1 | Out-Null
            }
        }
        git pull --rebase --autostash 2>&1 | Out-Null
        git add $statsFile
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        $pushResult = git push 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Git push greška: $pushResult" -ForegroundColor Red
        } else {
            Write-Host "Push završen." -ForegroundColor Green
        }
    }
    catch {
        Write-Host "GLAVNA GREŠKA: $($_.Exception.Message)" -ForegroundColor Red
    }
    Start-Sleep -Seconds 60
}
