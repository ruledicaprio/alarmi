# BHT Alarm Engine v29 - FINAL WORKING VERSION
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta već radi." -ForegroundColor Red; exit }

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio"
git config user.email "tvoja-email@adresa.com"

$previousCount = 0
if (Test-Path $counterFile) { $previousCount = [int](Get-Content $counterFile) }

# Funkcija za parsiranje datuma - podržava sve tvoje formate
function ConvertTo-DateTime($dateStr) {
    if ([string]::IsNullOrWhiteSpace($dateStr)) { return $null }
    $dateStr = $dateStr.Trim()
    # Zamijeni podvlaku razmakom (za Ignition i DSE)
    $dateStr = $dateStr -replace '_', ' '
    # Ukloni vremensku zonu ako postoji
    $dateStr = $dateStr -replace '\s+[+-]\d{4}$', ''
    $dateStr = $dateStr -replace '\s+[+-]\d{2}:?\d{2}$', ''
    
    # Prvo pokušaj sa formatom "dd MMM yyyy HH:mm:ss" (npr. 14 Apr 2026 13:25:02)
    $match = [regex]::Match($dateStr, '^(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})')
    if ($match.Success) {
        $day = $match.Groups[1].Value
        $month = $match.Groups[2].Value
        $year = $match.Groups[3].Value
        $time = $match.Groups[4].Value
        $monthNum = @{
            Jan=1;Feb=2;Mar=3;Apr=4;May=5;Jun=6;Jul=7;Aug=8;Sep=9;Oct=10;Nov=11;Dec=12
        }[$month]
        return [DateTime]::new([int]$year, $monthNum, [int]$day, [int]$time.Substring(0,2), [int]$time.Substring(3,2), [int]$time.Substring(6,2))
    }
    
    # Zatim pokušaj sa formatom "yyyy-MM-dd HH:mm:ss"
    try {
        return [DateTime]::ParseExact($dateStr, "yyyy-MM-dd HH:mm:ss", $null)
    } catch {}
    
    # Na kraju opći parse
    try {
        return [DateTime]::Parse($dateStr)
    } catch {
        return $null
    }
}

while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $weekAgo = $today.AddDays(-7)
        $allEvents = @()
        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..."

        # ========== CSV IZVOR ==========
        $csvCount = 0
        try {
            $csvResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap" -UseBasicParsing -TimeoutSec 30
            # Važno: .Content je već string, ne pretvaraj ga ponovo
            $csvContent = $csvResponse.Content
            $csvLines = $csvContent -split "`r?`n"
            
            foreach ($line in $csvLines) {
                if ([string]::IsNullOrWhiteSpace($line)) { continue }
                if ($line -notlike "*,*") { continue }
                
                $parts = $line.Split(',').ForEach({ $_.Trim().Trim('"') })
                if ($parts.Count -lt 3) { continue }
                
                # Pronađi kolonu sa datumom (sadrži 4 cifre, crticu, itd.)
                $timeIdx = -1
                for ($i=0; $i -lt $parts.Count; $i++) {
                    if ($parts[$i] -match '\d{4}-\d{2}-\d{2}[ _]\d{2}:\d{2}:\d{2}') {
                        $timeIdx = $i
                        break
                    }
                }
                if ($timeIdx -eq -1) { continue }
                
                $system = $parts[0]
                $siteRaw = $parts[1]
                $site = $siteRaw.ToUpper().Replace(" ", "_")
                $alarm = $parts[2]
                $timeStr = $parts[$timeIdx]
                $time = ConvertTo-DateTime $timeStr
                if ($time -eq $null) { continue }
                
                if ($system -eq "IgnitionSCADA") {
                    $status = "UNKNOWN"
                } else {
                    $isCleared = $line -match 'clear|normal|ok|Stops|UsageNormal'
                    $status = if ($isCleared) { "CLEARED" } else { "ACTIVE" }
                }
                
                $allEvents += [PSCustomObject]@{
                    System = $system
                    Site = $site
                    Alarm = $alarm
                    Time = $time
                    Status = $status
                }
                $csvCount++
            }
            Write-Host "  CSV: $csvCount događaja"
        } catch {
            Write-Host "  CSV greška: $($_.Exception.Message)" -ForegroundColor Red
        }

        # ========== HTML IZVOR (tabela sa statusima) ==========
        $htmlCount = 0
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = $htmlResponse.Content
            $lines = $htmlContent -split "`r?`n"
            $currentSection = "NETWORK"
            
            foreach ($line in $lines) {
                # Detekcija sekcija (---MPLS---, ---BTS---, itd.)
                if ($line -match '---+\s*([A-Z]+)\s*---+') {
                    $currentSection = $Matches[1].Trim()
                    continue
                }
                
                # Traži red koji sadrži datum u formatu "dd MMM yyyy HH:mm:ss"
                $dateMatch = [regex]::Match($line, '(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})')
                if (-not $dateMatch.Success) {
                    # Pokušaj sa formatom "yyyy-MM-dd HH:mm:ss"
                    $dateMatch = [regex]::Match($line, '(\d{4}-\d{2}-\d{2})\s+(\d{2}:\d{2}:\d{2})')
                    if ($dateMatch.Success) {
                        $dateMatch = [regex]::Match($line, '(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})')
                    }
                }
                
                if ($dateMatch.Success) {
                    $dateStr = $dateMatch.Value
                    $time = ConvertTo-DateTime $dateStr
                    if ($time -eq $null) { continue }
                    
                    # Izdvoji naziv site-a (sve ispred datuma)
                    $idx = $line.IndexOf($dateStr)
                    if ($idx -eq -1) { continue }
                    $beforeDate = $line.Substring(0, $idx).Trim()
                    # Očisti HTML tagove i entitete
                    $siteRaw = $beforeDate -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' ' -replace '^\s+|\s+$', ''
                    if (-not [string]::IsNullOrWhiteSpace($siteRaw) -and $siteRaw -notmatch '^-+$') {
                        $siteNorm = $siteRaw.ToUpper().Replace(" ", "_")
                        $allEvents += [PSCustomObject]@{
                            System = $currentSection
                            Site = $siteNorm
                            Alarm = "NE is Disconnected"
                            Time = $time
                            Status = "ACTIVE"
                        }
                        $htmlCount++
                    }
                }
            }
            Write-Host "  HTML: $htmlCount događaja"
        } catch {
            Write-Host "  HTML greška: $($_.Exception.Message)" -ForegroundColor Red
        }

        Write-Host "  UKUPNO: $($allEvents.Count) događaja"

        # ========== AGREGACIJA (ista kao prije, samo skraćeno) ==========
        function Get-DurationInInterval($events, $startDate, $endDate) {
            $grouped = $events | Where-Object { $_.System -ne "IgnitionSCADA" } | Group-Object Site, Alarm
            $result = @()
            foreach ($grp in $grouped) {
                $sorted = $grp.Group | Sort-Object Time
                $totalDur = 0
                $activeStart = $null
                $lastStatus = "CLEARED"
                for ($i=0; $i -lt $sorted.Count; $i++) {
                    $e = $sorted[$i]
                    if ($e.Status -eq "ACTIVE") {
                        $activeStart = $e.Time
                        $lastStatus = "ACTIVE"
                    } elseif ($e.Status -eq "CLEARED" -and $activeStart -ne $null) {
                        $clearTime = $e.Time
                        $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
                        $overlapEnd   = if ($clearTime -lt $endDate) { $clearTime } else { $endDate }
                        if ($overlapEnd -gt $overlapStart) {
                            $totalDur += ($overlapEnd - $overlapStart).TotalMinutes
                        }
                        $activeStart = $null
                        $lastStatus = "CLEARED"
                    }
                }
                if ($activeStart -ne $null) {
                    $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
                    $overlapEnd = $endDate
                    if ($overlapEnd -gt $overlapStart) {
                        $totalDur += ($overlapEnd - $overlapStart).TotalMinutes
                    }
                    $lastStatus = "ACTIVE"
                }
                $cnt = ($sorted | Where-Object { $_.Status -eq "ACTIVE" -and $_.Time -ge $startDate -and $_.Time -le $endDate }).Count
                $result += [PSCustomObject]@{
                    Site = $grp.Values[0]
                    Alarm = $grp.Values[1]
                    System = $sorted[0].System
                    Count = $cnt
                    Duration = [Math]::Round($totalDur, 1)
                    LastStatus = $lastStatus
                }
            }
            $ignEvents = $events | Where-Object { $_.System -eq "IgnitionSCADA" -and $_.Time -ge $startDate -and $_.Time -le $endDate }
            $ignGrouped = $ignEvents | Group-Object Site, Alarm | ForEach-Object {
                [PSCustomObject]@{
                    Site = $_.Values[0]
                    Alarm = $_.Values[1]
                    System = "IgnitionSCADA"
                    Count = $_.Count
                    Duration = 0
                    LastStatus = "UNKNOWN"
                }
            }
            return ($result + $ignGrouped)
        }

        $dailyAgg = Get-DurationInInterval $allEvents $today $now
        $weeklyAgg = Get-DurationInInterval $allEvents $weekAgo $now

        # Spajanje dnevnih i sedmičnih
        $allStats = @{}
        foreach ($d in $dailyAgg) {
            $key = "$($d.Site)|$($d.Alarm)|$($d.System)"
            $allStats[$key] = @{
                Site = $d.Site; Alarm = $d.Alarm; System = $d.System
                DayCnt = $d.Count; DayDur = $d.Duration
                WeekCnt = 0; WeekDur = 0
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
                    Site = $w.Site; Alarm = $w.Alarm; System = $w.System
                    DayCnt = 0; DayDur = 0
                    WeekCnt = $w.Count; WeekDur = $w.Duration
                    LastStatus = $w.LastStatus
                }
            }
        }

        $finalStats = $allStats.Values | ForEach-Object {
            [PSCustomObject]@{
                Site = $_.Site; Alarm = $_.Alarm; System = $_.System
                DayCnt = $_.DayCnt; DayDur = $_.DayDur
                WeekCnt = $_.WeekCnt; WeekDur = $_.WeekDur
                LastStatus = $_.LastStatus
                Region = "N/A"
            }
        }

        # Ispis u konzolu
        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v29 - $($now.ToString('HH:mm:ss'))"
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih događaja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 5 DNEVNIH ISPADADA:"
        $dailyAgg | Where-Object { $_.Duration -gt 0 } | Sort-Object Duration -Descending | Select-Object -First 5 | ForEach-Object {
            Write-Host "  $($_.Site) - $($_.Duration) min"
        }

        # JSON izlaz
        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $finalStats
            Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500
        }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        # Git push
        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        git add $statsFile
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        git pull --rebase --autostash 2>&1 | Out-Null
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
