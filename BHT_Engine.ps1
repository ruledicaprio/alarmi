# BHT Alarm Engine v24 - fixed git order, robust aggregates
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta već radi." -ForegroundColor Red; exit }

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio"
git config user.email "rusmirskopljak@gmail.com"   # ZAMIJENI

$previousCount = 0
if (Test-Path $counterFile) { $previousCount = [int](Get-Content $counterFile) }

while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $weekAgo = $today.AddDays(-7)
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
                if ($system -eq "Ignition") { $status = "UNKNOWN" }
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
                        $allEvents += [PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE is Disconnected"; Time=$time; Status="ACTIVE" }
                    }
                }
            }
        } catch { Write-Host "  HTML greška: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host "  Ukupno događaja: $($allEvents.Count)"

        # --- Funkcija za agregaciju (dnevnu ili sedmičnu) ---
        function Get-Aggregates($events, $startDate, $endDate) {
        # events su svi događaji (nefiltrirani)
        # startDate i endDate definišu interval za agregaciju
        $grouped = $events | Where-Object { $_.System -ne "Ignition" } | Group-Object Site, Alarm | ForEach-Object {
            $g = $_.Group | Sort-Object Time
            $totalDur = 0
            $totalCnt = 0
            $lastStatus = "CLEARED"
            $i = 0
            while ($i -lt $g.Count) {
                $current = $g[$i]
                if ($current.Status -eq "ACTIVE") {
                    # Potraži odgovarajući CLEARED (prvi nakon trenutnog)
                    $clearTime = $endDate  # default do kraja intervala
                    $j = $i + 1
                    while ($j -lt $g.Count) {
                        if ($g[$j].Status -eq "CLEARED") {
                            $clearTime = $g[$j].Time
                            break
                        }
                        $j++
                    }
                    # Izračunaj stvarno trajanje unutar intervala
                    $start = $current.Time
                    $end = $clearTime
                    # Ograniči na interval [startDate, endDate]
                    if ($end -gt $endDate) { $end = $endDate }
                    if ($start -lt $startDate) { $start = $startDate }
                    if ($end > $start) {
                        $duration = ($end - $start).TotalMinutes
                        $totalDur += $duration
                        $totalCnt++
                    }
                    # Pomjeri i na CLEARED (ako postoji) da ne brojimo ponovo
                    if ($clearTime -ne $endDate) {
                        # Pronađi index CLEARED događaja
                        for ($k=$i+1; $k -lt $g.Count; $k++) {
                            if ($g[$k].Status -eq "CLEARED") {
                                $i = $k
                                break
                            }
                        }
                    }
                    $lastStatus = "ACTIVE"
                } else {
                    $lastStatus = "CLEARED"
                }
                $i++
            }
            # Nakon petlje, ako je posljednji status ACTIVE, računaj do kraja intervala
            if ($lastStatus -eq "ACTIVE") {
                $lastEvent = $g[-1]
                if ($lastEvent.Time -lt $endDate) {
                    $start = [Math]::Max($lastEvent.Time, $startDate)
                    if ($start -lt $endDate) {
                        $duration = ($endDate - $start).TotalMinutes
                        $totalDur += $duration
                        $totalCnt++
                    }
                }
            }
            [PSCustomObject]@{
                Site = $_.Values[0]
                Alarm = $_.Values[1]
                System = $g[0].System
                Count = $totalCnt
                Duration = [Math]::Round($totalDur,1)
                LastStatus = $lastStatus
            }
        }
        # Ignition (samo broj)
        $ignEvents = $events | Where-Object { $_.System -eq "Ignition" -and $_.Time -ge $startDate -and $_.Time -le $endDate }
        $ignGrouped = $ignEvents | Group-Object Site, Alarm | ForEach-Object {
            [PSCustomObject]@{
                Site = $_.Values[0]
                Alarm = $_.Values[1]
                System = "Ignition"
                Count = $_.Count
                Duration = 0
                LastStatus = "UNKNOWN"
            }
        }
        return ($grouped + $ignGrouped)
    }

        # Dnevni agregati (od ponoći do sada)
        $dailyAgg = Get-Aggregates $allEvents $today $now
        # Sedmični agregati (posljednjih 7 dana do sada)
        $weeklyAgg = Get-Aggregates $allEvents $weekAgo $now

        # Spajanje u jednu listu (kombinujemo po (Site, Alarm, System))
        $allStats = @{}
        foreach ($d in $dailyAgg) {
            $key = "$($d.Site)|$($d.Alarm)|$($d.System)"
            $allStats[$key] = @{
                Site = $d.Site
                Alarm = $d.Alarm
                System = $d.System
                DayCnt = $d.Count
                DayDur = $d.Duration
                WeekCnt = 0
                WeekDur = 0
                LastStatus = $d.LastStatus
            }
        }
        foreach ($w in $weeklyAgg) {
            $key = "$($w.Site)|$($w.Alarm)|$($w.System)"
            if ($allStats.ContainsKey($key)) {
                $allStats[$key].WeekCnt = $w.Count
                $allStats[$key].WeekDur = $w.Duration
            } else {
                $allStats[$key] = @{
                    Site = $w.Site
                    Alarm = $w.Alarm
                    System = $w.System
                    DayCnt = 0
                    DayDur = 0
                    WeekCnt = $w.Count
                    WeekDur = $w.Duration
                    LastStatus = $w.LastStatus
                }
            }
        }

        $finalStats = $allStats.Values | ForEach-Object {
            [PSCustomObject]@{
                Site = $_.Site
                Alarm = $_.Alarm
                System = $_.System
                DayCnt = $_.DayCnt
                DayDur = $_.DayDur
                WeekCnt = $_.WeekCnt
                WeekDur = $_.WeekDur
                LastStatus = $_.LastStatus
                Region = "N/A"   # region se dodjeljuje na klijentu
            }
        }

        # --- Ispis u konzolu ---
        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v24 - $($now.ToString('HH:mm:ss'))"
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih događaja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 5 DNEVNIH ISPADADA:"
        $dailyAgg | Where-Object { $_.Duration -gt 0 } | Sort-Object Duration -Descending | Select-Object -First 5 | ForEach-Object {
            Write-Host "  $($_.Site) - $($_.Duration) min"
        }

        # --- JSON izlaz ---
        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $finalStats
            Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500
        }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        # --- Git sinhronizacija (ispravljen redoslijed) ---
        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        # Prvo lokalno dodaj i komituj
        git add $statsFile
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        # Zatim povuci remote promjene i rebase
        git pull --rebase --autostash 2>&1 | Out-Null
        # Na kraju push
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