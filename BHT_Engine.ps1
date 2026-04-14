# BHT Alarm Engine v31 - FINAL WORKING
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta već radi." -ForegroundColor Red; exit }

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio"
git config user.email "rusmirskopljak@gmail.com"

$previousCount = 0
if (Test-Path $counterFile) { $previousCount = [int](Get-Content $counterFile) }

function ConvertTo-DateTime($dateStr) {
    if ([string]::IsNullOrWhiteSpace($dateStr)) { return $null }
    $dateStr = $dateStr.Trim()
    $dateStr = $dateStr -replace '_', ' '
    $dateStr = $dateStr -replace '\s+[+-]\d{2}:?\d{2}$', ''
    $match = [regex]::Match($dateStr, '^(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})')
    if ($match.Success) {
        $day = [int]$match.Groups[1].Value
        $monthName = $match.Groups[2].Value
        $year = [int]$match.Groups[3].Value
        $time = $match.Groups[4].Value
        $month = @{Jan=1;Feb=2;Mar=3;Apr=4;May=5;Jun=6;Jul=7;Aug=8;Sep=9;Oct=10;Nov=11;Dec=12}[$monthName]
        $hour = [int]$time.Substring(0,2)
        $minute = [int]$time.Substring(3,2)
        $second = [int]$time.Substring(6,2)
        return [DateTime]::new($year, $month, $day, $hour, $minute, $second)
    }
    try { return [DateTime]::ParseExact($dateStr, "yyyy-MM-dd HH:mm:ss", $null) } catch {}
    try { return [DateTime]::Parse($dateStr) } catch { return $null }
}

while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $weekAgo = $today.AddDays(-7)
        $allEvents = @()
        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..."

        # --- CSV ---
        $csvCount = 0
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
                $time = ConvertTo-DateTime ($p[$tsIdx] -replace '_', ' ')
                if ($time -eq $null) { continue }
                if ($system -eq "IgnitionSCADA") { $status = "UNKNOWN" }
                else { $status = if ($line -match 'clear|normal|ok|Stops|UsageNormal') { "CLEARED" } else { "ACTIVE" } }
                $allEvents += [PSCustomObject]@{ System=$system; Site=$site; Alarm=$alarm; Time=$time; Status=$status }
                $csvCount++
            }
            Write-Host "  CSV: $csvCount događaja"
        } catch { Write-Host "  CSV greška: $($_.Exception.Message)" -ForegroundColor Red }

        # --- HTML ---
        $htmlCount = 0
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = [System.Text.Encoding]::UTF8.GetString($htmlResponse.Content)
            $lines = $htmlContent -split "`r?`n"
            $currentSection = "NETWORK"
            foreach ($line in $lines) {
                if ($line -match '---+\s*([A-Z]+)\s*---+') { $currentSection = $Matches[1].Trim(); continue }
                if ($line -match '\b(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})') {
                    $dateStr = "$($Matches[1]) $($Matches[2]) $($Matches[3]) $($Matches[4])"
                    $time = ConvertTo-DateTime $dateStr
                    if ($time -eq $null) { continue }
                    $idx = $line.IndexOf($dateStr)
                    if ($idx -eq -1) { continue }
                    $beforeDate = $line.Substring(0, $idx).Trim()
                    $siteRaw = $beforeDate -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' ' -replace '^\s+|\s+$', ''
                    if (-not [string]::IsNullOrWhiteSpace($siteRaw) -and $siteRaw -notmatch '^-+$') {
                        $siteNorm = $siteRaw.ToUpper().Replace(" ", "_")
                        $allEvents += [PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE is Disconnected"; Time=$time; Status="ACTIVE" }
                        $htmlCount++
                    }
                } elseif ($line -match '\b(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2})\b') {
                    $dateStr = $Matches[1]
                    $time = ConvertTo-DateTime $dateStr
                    if ($time -eq $null) { continue }
                    $idx = $line.IndexOf($dateStr)
                    if ($idx -eq -1) { continue }
                    $beforeDate = $line.Substring(0, $idx).Trim()
                    $siteRaw = $beforeDate -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' ' -replace '^\s+|\s+$', ''
                    if (-not [string]::IsNullOrWhiteSpace($siteRaw) -and $siteRaw -notmatch '^-+$') {
                        $siteNorm = $siteRaw.ToUpper().Replace(" ", "_")
                        $allEvents += [PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE is Disconnected"; Time=$time; Status="ACTIVE" }
                        $htmlCount++
                    }
                }
            }
            Write-Host "  HTML: $htmlCount događaja"
        } catch { Write-Host "  HTML greška: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host "  UKUPNO: $($allEvents.Count) događaja"

        # --- Agregacija ---
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
                        if ($overlapEnd -gt $overlapStart) { $totalDur += ($overlapEnd - $overlapStart).TotalMinutes }
                        $activeStart = $null
                        $lastStatus = "CLEARED"
                    }
                }
                if ($activeStart -ne $null) {
                    $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
                    $overlapEnd = $endDate
                    if ($overlapEnd -gt $overlapStart) { $totalDur += ($overlapEnd - $overlapStart).TotalMinutes }
                }
                $cnt = ($sorted | Where-Object { $_.Status -eq "ACTIVE" -and $_.Time -ge $startDate -and $_.Time -le $endDate }).Count
                $result += [PSCustomObject]@{
                    Site = $grp.Values[0]; Alarm = $grp.Values[1]; System = $sorted[0].System
                    Count = $cnt; Duration = [Math]::Round($totalDur, 1); LastStatus = $lastStatus
                }
            }
            $ignEvents = $events | Where-Object { $_.System -eq "IgnitionSCADA" -and $_.Time -ge $startDate -and $_.Time -le $endDate }
            $ignGrouped = $ignEvents | Group-Object Site, Alarm | ForEach-Object {
                [PSCustomObject]@{ Site = $_.Values[0]; Alarm = $_.Values[1]; System = "IgnitionSCADA"; Count = $_.Count; Duration = 0; LastStatus = "UNKNOWN" }
            }
            return ($result + $ignGrouped)
        }

        $dailyAgg = Get-DurationInInterval $allEvents $today $now
        $weeklyAgg = Get-DurationInInterval $allEvents $weekAgo $now

        $allStats = @{}
        foreach ($d in $dailyAgg) { $key = "$($d.Site)|$($d.Alarm)|$($d.System)"; $allStats[$key] = @{ Site=$d.Site; Alarm=$d.Alarm; System=$d.System; DayCnt=$d.Count; DayDur=$d.Duration; WeekCnt=0; WeekDur=0; LastStatus=$d.LastStatus } }
        foreach ($w in $weeklyAgg) { $key = "$($w.Site)|$($w.Alarm)|$($w.System)"; if ($allStats.ContainsKey($key)) { $allStats[$key].WeekCnt = $w.Count; $allStats[$key].WeekDur = $w.Duration } else { $allStats[$key] = @{ Site=$w.Site; Alarm=$w.Alarm; System=$w.System; DayCnt=0; DayDur=0; WeekCnt=$w.Count; WeekDur=$w.Duration; LastStatus=$w.LastStatus } } }

        $finalStats = $allStats.Values | ForEach-Object { [PSCustomObject]@{ Site=$_.Site; Alarm=$_.Alarm; System=$_.System; DayCnt=$_.DayCnt; DayDur=$_.DayDur; WeekCnt=$_.WeekCnt; WeekDur=$_.WeekDur; LastStatus=$_.LastStatus; Region="N/A" } }

        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v31 - $($now.ToString('HH:mm:ss'))"
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih događaja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 5 DNEVNIH ISPADADA:"
        $dailyAgg | Where-Object { $_.Duration -gt 0 } | Sort-Object Duration -Descending | Select-Object -First 5 | ForEach-Object { Write-Host "  $($_.Site) - $($_.Duration) min" }

        $output = @{ LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss"); Stats = $finalStats; Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500 }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        git add $statsFile
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        git pull --rebase --autostash 2>&1 | Out-Null
        $pushResult = git push 2>&1
        if ($LASTEXITCODE -ne 0) { Write-Host "Git push greška: $pushResult" -ForegroundColor Red }
        else { Write-Host "Push završen." -ForegroundColor Green }
    }
    catch { Write-Host "GLAVNA GREŠKA: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}