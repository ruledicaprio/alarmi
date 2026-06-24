# BHT Alarm Engine v3.2 - FINAL PRODUCTION (PowerShell 5.1 Compatible)
$ErrorActionPreference = 'SilentlyContinue'
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { 
    Write-Host "Skripta već radi." -ForegroundColor Red
    exit 
}

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio" 2>$null | Out-Null
git config user.email "rusmirskopljak@gmail.com" 2>$null | Out-Null

$previousCount = 0
if (Test-Path $counterFile) { 
    try { $previousCount = [int](Get-Content $counterFile -Raw) } 
    catch { $previousCount = 0 } 
}

# =========================================================
# HELPER FUNKCIJE
# =========================================================
function ConvertTo-DateTime {
    param([string]$dateStr)
    if ([string]::IsNullOrWhiteSpace($dateStr)) { return $null }
    $clean = $dateStr.Trim() -replace '_', ' ' -replace '\s+[+-]\d{2}:?\d{2}$', ''
    try { return [DateTime]::ParseExact($clean, "dd MMM yyyy HH:mm:ss", [System.Globalization.CultureInfo]::InvariantCulture) } catch {}
    try { return [DateTime]::ParseExact($clean, "yyyy-MM-dd HH:mm:ss", $null) } catch {}
    try { return [DateTime]::Parse($clean) } catch { return $null }
}

function Get-RegionFromSite {
    param([string]$Site)
    $s = $Site.Trim().ToUpper()
    
    # 1. Čišćenje prefiksa
    $s = $s -replace '^(BTS_|BS_|RRST_)', ''
    
    # 2. Eksplicitna mapa (najviši prioritet)
    $explicitMap = @{
        'GRABOVICA'       = 'Mostar'
        'GRABOVICA_TUZLA' = 'Tuzla'
        'TUZLA_KISELJAK'  = 'Tuzla'
        'KISELJAK_CENTAR' = 'Travnik'
        'POSUSJE_OSREDAK' = 'Mostar'
        'POSUSJE_CENTAR'  = 'Mostar'
        'MANJACA'         = 'Travnik'
        'KMUR'            = 'Goražde'
        'CELINAC_BOJICI'  = 'Zenica'
        'CELINAC_JOSAVKA' = 'Zenica'
    }
    if ($explicitMap.ContainsKey($s)) { return $explicitMap[$s] }

    # 3. Suffix/Prefix pravila
    if ($s -match '_TUZLA$|^TUZLA_') { return 'Tuzla' }
    if ($s -match '_SARAJEVO$|^SARAJEVO_') { return 'Sarajevo' }
    if ($s -match '_ZENICA$|^ZENICA_') { return 'Zenica' }
    if ($s -match '_MOSTAR$|^MOSTAR_') { return 'Mostar' }
    if ($s -match '_BIHAC$|^BIHAC_') { return 'Bihać' }
    if ($s -match '_TRAVNIK$|^TRAVNIK_') { return 'Travnik' }
    if ($s -match '_GORAZDE$|^GORAZDE_') { return 'Goražde' }

    # 4. Fuzzy match
    if ($s -match 'SARAJEVO|ILIDZA|VOGOSCA|ALIPASINO|HRASNICA|KOBILJACA|GLADNO_POLJE|STUP|MISEVICI|HALILOVICI|DMALTA|OBALA|BASCARSIJA') { return 'Sarajevo' }
    if ($s -match 'TUZLA|GRAČANICA|LUKAVAC|KALESIJA|TISCA|KLJESTANI|BIJELJINA|ZVORNIK|SREBRENIK|TETIMA|JELASKE') { return 'Tuzla' }
    if ($s -match 'ZENICA|KAKANJ|VISOKO|ZAVIDOVICI|TEŠANJ|VAREŠ|BREZA|OLOVO|ŽEPČE|ZEPCE|STUPARI|KAKANJ|PUHOVI|NEMILA') { return 'Zenica' }
    if ($s -match 'MOSTAR|ČAPLJINA|ŠIROKI|GRUDE|LJUBUŠKI|KONJIC|JABLANICA|POSUSJE|PROZOR|PAPRASKO') { return 'Mostar' }
    if ($s -match 'BIHAC|CAZIN|VELIKA_KLADUSA|SANSKI_MOST|KLJUC|BOSANSKI_NOVI|DRAKSENIC|BUNAREVI|OSTROZAC|IZACIC') { return 'Bihać' }
    if ($s -match 'TRAVNIK|DVAKUF|JAJCE|VITEZ|BUGOJNO|GORNJI_VAKUF|NOVI_TRAVNIK|KAKRINJE') { return 'Travnik' }
    if ($s -match 'GORAZDE|FOCA|CAJNICE|RUDO|ROGATICA|USTIKOLINA|JOSANICA') { return 'Goražde' }
    if ($s -match 'BANJA_LUKA|GRADISKA|PRNJAVOR|CELINAC|STRICICI|MANJACA|NOVI_SEHER|LJUBIC') { return 'Banja Luka' }

    return 'Ostalo'
}

function Get-DurationInInterval {
    param(
        [System.Collections.ArrayList]$events,
        [DateTime]$startDate,
        [DateTime]$endDate
    )

    $grouped = $events | Where-Object { $_.System -ne "IgnitionSCADA" } | Group-Object { "$($_.System)|$($_.Site)|$($_.Alarm)" }
    $result = @()

    foreach ($grp in $grouped) {
        $sorted = $grp.Group | Sort-Object Time
        $totalDur = 0
        $activeStart = $null
        $count = 0

        foreach ($e in $sorted) {
            if ($e.Time -lt $startDate -or $e.Time -gt $endDate) { continue }

            if ($e.Status -match 'ACTIVE|MAJOR|CRITICAL') {
                if ($null -eq $activeStart) {
                    $activeStart = $e.Time
                    $count++
                }
            }
            elseif ($e.Status -match 'CLEARED|MINOR|NORMAL') {
                if ($activeStart) {
                    $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
                    $overlapEnd   = if ($e.Time -lt $endDate) { $e.Time } else { $endDate }
                    if ($overlapEnd -gt $overlapStart) {
                        $totalDur += ($overlapEnd - $overlapStart).TotalMinutes
                    }
                    $activeStart = $null
                }
            }
        }

        # Ako je alarm još aktivan na kraju intervala
        if ($activeStart) {
            $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
            $totalDur += ($endDate - $overlapStart).TotalMinutes
        }

        if ($totalDur -gt 0 -or $count -gt 0) {
            $parts = $grp.Name -split '\|'
            $result += [PSCustomObject]@{
                System     = $parts[0]
                Site       = $parts[1]
                Alarm      = $parts[2]
                Region     = $sorted[0].Region
                DayCnt     = $count
                DayDur     = [Math]::Round($totalDur, 1)
                LastStatus = if ($activeStart) { "ACTIVE" } else { "CLEARED" }
            }
        }
    }

    # Posebno za IgnitionSCADA (nema duration, samo count)
    $ignEvents = $events | Where-Object { $_.System -eq "IgnitionSCADA" -and $_.Time -ge $startDate -and $_.Time -le $endDate }
    $ignGrouped = $ignEvents | Group-Object { "$($_.Site)|$($_.Alarm)" } | ForEach-Object {
        $parts = $_.Name -split '\|'
        [PSCustomObject]@{
            System     = "IgnitionSCADA"
            Site       = $parts[0]
            Alarm      = $parts[1]
            Region     = $_.Group[0].Region
            DayCnt     = $_.Count
            DayDur     = 0
            LastStatus = "UNKNOWN"
        }
    }

    return $result + $ignGrouped
}

# =========================================================
# UNIVERZALNI PARSER
# =========================================================
function Parse-AlarmLine {
    param([string]$Line)
    
    if ([string]::IsNullOrWhiteSpace($Line) -or $Line -notlike "*,*") { return $null }
    
    $parts = $Line.Split(',').ForEach({ $_.Trim().Trim('"') }) | Where-Object { $_ }
    if ($parts.Count -lt 4) { return $null }

    $alarm = [PSCustomObject]@{
        System = ''; Site = ''; Alarm = ''; Status = ''; Time = $null; Region = 'N/A'; IP = ''
    }

    # IGNITION SCADA
    if ($parts[0] -eq 'IgnitionSCADA') {
        $alarm.System = 'IgnitionSCADA'
        $fullSite = $parts[1]
        if ($fullSite -match '^([A-Za-zŠšĐđČčĆćŽž]+)\s*-\s*(.+)$') {
            $alarm.Region = $Matches[1].Trim()
            $alarm.Site = $Matches[2].Trim().ToUpper().Replace(' ', '_').Replace('-','_')
        } else {
            $alarm.Site = $fullSite.ToUpper().Replace(' ', '_').Replace('-','_')
            $alarm.Region = Get-RegionFromSite $alarm.Site
        }
        $alarm.Alarm = $parts[2]
        $alarm.Status = if ($parts.Count -gt 4) { $parts[4].Trim().ToUpper() } else { "UNKNOWN" }
        $ts = $parts[5].Trim() -replace '_', ' '
        $alarm.Time = ConvertTo-DateTime $ts
    }
    # NETECO
    elseif ($parts[0] -eq 'NetEco') {
        $alarm.System = 'NetEco'
        $alarm.Site = $parts[1].Trim().ToUpper()
        $alarm.Alarm = $parts[2].Trim()
        $alarm.Time = ConvertTo-DateTime ($parts[3].Trim())
        $alarm.Status = if ($parts.Count -gt 4) { $parts[4].Trim().ToUpper() } else { "ACTIVE" }
        $alarm.Region = Get-RegionFromSite $alarm.Site
    }
    # RPS-SC200/300, DSE, BARAN, EATON, RITTAL
    elseif ($parts[0] -match 'RpsSc300Mib|RPS-SC200-MIB|DSE-74xx|BARAN|EATON|RITTAL') {
        $alarm.System = $parts[0].Trim()
        $alarm.Site = $parts[1].Trim().ToUpper()
        $alarm.Region = $parts[2].Trim()
        $alarm.Alarm = $parts[3].Trim() -replace '_', ' '
        $alarm.Status = $parts[-1].Trim().ToUpper()
        $ts = $parts[5].Trim() -replace '_', ' '
        $alarm.Time = ConvertTo-DateTime $ts
        if ($parts.Count -gt 7) { $alarm.IP = $parts[6].Trim() }
    }
    # U2020
    elseif ($parts[0] -eq 'U2020') {
        $alarm.System = 'U2020'
        $alarm.Site = $parts[1].Trim().ToUpper()
        $alarm.Alarm = $parts[2].Trim()
        $alarm.Time = ConvertTo-DateTime ($parts[3].Trim())
        $alarm.Status = if ($parts.Count -gt 4) { $parts[4].Trim().ToUpper() } else { "ACTIVE" }
        $alarm.Region = Get-RegionFromSite $alarm.Site
    }

    if ($null -eq $alarm.Time) { return $null }
    return $alarm
}

# =========================================================
# MAIN LOOP
# =========================================================
while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $weekAgo = $today.AddDays(-7)
        $allEvents = [System.Collections.ArrayList]::new()

        [Console]::OutputEncoding = [System.Text.Encoding]::UTF8
        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v3.2 - $($now.ToString('yyyy-MM-dd HH:mm:ss'))" -ForegroundColor White
        Write-Host "============================================================" -ForegroundColor Gray

        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..." -ForegroundColor Cyan

        # CSV PARSE
        $csvCount = 0
        try {
            $csvResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap" -UseBasicParsing -TimeoutSec 30
            $csvLines = [System.Text.Encoding]::UTF8.GetString($csvResponse.Content) -split "`r?`n"
            foreach ($line in $csvLines) {
                if ([string]::IsNullOrWhiteSpace($line) -or $line -notlike "*,*") { continue }
                $parsed = Parse-AlarmLine $line
                if ($parsed) {
                    $allEvents.Add($parsed) | Out-Null
                    $csvCount++
                }
            }
            Write-Host " CSV: $csvCount događaja" -ForegroundColor Green
        } catch { Write-Host " CSV greška: $($_.Exception.Message)" -ForegroundColor Red }

        # HTML PARSE
        $htmlCount = 0
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = [System.Text.Encoding]::UTF8.GetString($htmlResponse.Content)
            $lines = $htmlContent -split "`r?`n"
            $currentSection = "NETWORK"

            foreach ($line in $lines) {
                if ($line -match '^\s*-+\s*([A-Z]+)\s*-+\s*$') {
                    $currentSection = $Matches[1].Trim()
                    continue
                }
                if ($line -match '\b(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})') {
                    $dateStr = "$($Matches[1]) $($Matches[2]) $($Matches[3]) $($Matches[4])"
                    $time = ConvertTo-DateTime $dateStr
                    if ($null -eq $time) { continue }

                    $idx = $line.IndexOf($dateStr)
                    if ($idx -gt 0) {
                        $siteRaw = $line.Substring(0, $idx).Trim() -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' '
                        if ($siteRaw -and $siteRaw -notmatch '^[-]+$') {
                            $siteNorm = $siteRaw.ToUpper().Replace(" ", "_").Replace("-", "_")
                            $allEvents.Add([PSCustomObject]@{
                                System = $currentSection
                                Site = $siteNorm
                                Alarm = "NE is Disconnected"
                                Time = $time
                                Status = "ACTIVE"
                                Region = Get-RegionFromSite $siteNorm
                            }) | Out-Null
                            $htmlCount++
                        }
                    }
                }
            }
            Write-Host " HTML: $htmlCount događaja" -ForegroundColor Green
        } catch { Write-Host " HTML greška: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host " UKUPNO: $($allEvents.Count) događaja" -ForegroundColor Yellow

        # Agregacija
        $dailyAgg = Get-DurationInInterval $allEvents $today $now
        $weeklyAgg = Get-DurationInInterval $allEvents $weekAgo $now

        $allStats = @{}
        foreach ($d in $dailyAgg) {
            $key = "$($d.Site)|$($d.Alarm)|$($d.System)"
            $allStats[$key] = [PSCustomObject]@{
                Site=$d.Site; Alarm=$d.Alarm; System=$d.System; Region=$d.Region
                DayCnt=$d.DayCnt; DayDur=$d.DayDur; WeekCnt=0; WeekDur=0; LastStatus=$d.LastStatus
            }
        }
        foreach ($w in $weeklyAgg) {
            $key = "$($w.Site)|$($w.Alarm)|$($w.System)"
            if ($allStats.ContainsKey($key)) {
                $allStats[$key].WeekCnt = $w.DayCnt
                $allStats[$key].WeekDur = $w.DayDur
            } else {
                $allStats[$key] = [PSCustomObject]@{
                    Site=$w.Site; Alarm=$w.Alarm; System=$w.System; Region=$w.Region
                    DayCnt=0; DayDur=0; WeekCnt=$w.DayCnt; WeekDur=$w.DayDur; LastStatus=$w.LastStatus
                }
            }
        }

        $finalStats = $allStats.Values | ForEach-Object {
            [PSCustomObject]@{
                System = $_.System
                Site = $_.Site
                Alarm = $_.Alarm
                Region = $_.Region
                LastStatus = $_.LastStatus
                DayCnt = [int]$_.DayCnt
                DayDur = [double]$_.DayDur
                WeekCnt = [int]$_.WeekCnt
                WeekDur = [double]$_.WeekDur
                MonthCnt = 0
                MonthDur = 0.0
                YearCnt = 0
                YearDur = 0.0
            }
        }

        # Novi događaji
        $newAlarms = [Math]::Max(0, $allEvents.Count - $previousCount)
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force

        Write-Host "`nNovih događaja: $newAlarms" -ForegroundColor Green
        Write-Host "`nTOP 20 DNEVNIH ISPADA NAPAJANJA:" -ForegroundColor Yellow
        $dailyAgg | Where-Object { $_.DayDur -gt 0 } | 
            Sort-Object DayDur -Descending | 
            Select-Object -First 20 | 
            ForEach-Object { Write-Host " $($_.Site) - $($_.DayDur) min" -ForegroundColor White }

        # ==================== JSON OUTPUT ====================
        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats      = $finalStats
            Recent     = $allEvents | Select-Object System, Site, Alarm, Time, Status, Region -First 500
        }

        $jsonString = $output | ConvertTo-Json -Depth 10

        # Čuvanje bez BOM-a
        try {
            $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
            [System.IO.File]::WriteAllText($statsFile, $jsonString, $utf8NoBom)
            Write-Host "`n[$($now.ToString('HH:mm:ss'))] stats_data.json uspješno ažuriran." -ForegroundColor Green
        } catch {
            Write-Host "`nGREŠKA pri upisu JSON-a: $($_.Exception.Message)" -ForegroundColor Red
        }

        # ==================== GIT SYNC ====================
        Write-Host "`nSinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        git add $statsFile 2>&1 | Out-Null
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        git pull --rebase --autostash 2>&1 | Out-Null
        $pushResult = git push 2>&1

        if ($LASTEXITCODE -eq 0) {
            Write-Host "Push završen." -ForegroundColor Green
        } else {
            Write-Host "Git push greška: $pushResult" -ForegroundColor Red
        }

        Write-Host "============================================================" -ForegroundColor Gray

    } catch {
        Write-Host "GLAVNA GREŠKA: $($_.Exception.Message)" -ForegroundColor Red
        Write-Host $_.ScriptStackTrace -ForegroundColor DarkRed
    }

    # Pauza 2 minute
    Start-Sleep -Seconds 120
}
