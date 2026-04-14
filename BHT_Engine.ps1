# BHT Alarm Engine v3.1 - FINAL PRODUCTION (PowerShell 5.1 Compatible)
$ErrorActionPreference = 'SilentlyContinue'
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta već radi." -ForegroundColor Red; exit }

$repoPath = "E:\BHT-Dashboard-Git"
Set-Location $repoPath
$statsFile = "$repoPath\stats_data.json"
$counterFile = "$repoPath\last_count.txt"

git config user.name "ruledicaprio" 2>$null | Out-Null
git config user.email "rusmirskopljak@gmail.com" 2>$null | Out-Null

$previousCount = 0
if (Test-Path $counterFile) { try { $previousCount = [int](Get-Content $counterFile -Raw) } catch { $previousCount = 0 } }

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
    
    # 1. Čišćenje prefiksa (BTS_, RRST_)
    $s = $s -replace '^(BTS_|BS_|RRST_)', ''
    
    # 2. Eksplicitna Mapa (Najviši prioritet)
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

    # 3. Suffix/Prefix Pravila

    $s = ($Site -replace '^(BTS_|RRST_|C\d+_|NOKIA\d+_)', '').Trim().ToUpper()
    $explicit = @{
        'GRABOVICA'='Mostar'; 'TUZLA_KISELJAK'='Tuzla'
        'KISELJAK_CENTAR'='Sarajevo'; 'POSUSJE_OSREDAK'='Mostar'; 'POSUSJE_CENTAR'='Mostar'
        'MANJACA'='Zenica'; 'KMUR'='GORAZDE_'; 'CELINAC_BOJICI'='Zenica'; 'CELINAC_JOSAVKA'='Zenica'
        'GRABOVICA_TUZLA'='Tuzla'
    }
    if ($explicit.ContainsKey($s)) { return $explicit[$s] }

    if ($s -match '_TUZLA$|^TUZLA_') { return 'Tuzla' }
    if ($s -match '_SARAJEVO$|^SARAJEVO_') { return 'Sarajevo' }
    if ($s -match '_ZENICA$|^ZENICA_') { return 'Zenica' }
    if ($s -match '_MOSTAR$|^MOSTAR_') { return 'Mostar' }
    if ($s -match '_BIHAC$|^BIHAC_') { return 'Bihać' }
    if ($s -match '_TRAVNIK$|^TRAVNIK_') { return 'Travnik' }
    if ($s -match '_GORAZDE$|^GORAZDE_') { return 'Goražde' }


    # 4. Fuzzy Match
    if ($s -match 'SARAJEVO|ILIDZA|VOGOSCA|ALIPASINO|HRASNICA|KOBILJACA|GLADNO_POLJE|STUP|MISEVICI|HALILOVICI') { return 'Sarajevo' }
    if ($s -match 'TUZLA|GRAČANICA|LUKAVAC|KALESIJA|TISCA|KLJESTANI|BIJELJINA|ZVORNIK|SREBRENIK') { return 'Tuzla' }
    if ($s -match 'ZENICA|KAKANJ|VISOKO|ZAVIDOVICI|TEŠANJ|VAREŠ|BREZA|OLOVO|ŽEPČE|ZEPCE|STUPARI') { return 'Zenica' }
    if ($s -match 'MOSTAR|ČAPLJINA|ŠIROKI|GRUDE|LJUBUŠKI|KONJIC|JABLANICA|POSUSJE|PROZOR') { return 'Mostar' }
    if ($s -match 'BIHAC|CAZIN|VELIKA_KLADUSA|SANSKI_MOST|KLJUC|BOSANSKI_NOVI|DRAKSENIC|BUNAREVI') { return 'Bihać' }
    if ($s -match 'TRAVNIK|DVAKUF|JAJCE|VITEZ|BUGOJNO|GORNJI_VAKUF|NOVI_TRAVNIK') { return 'Travnik' }
    if ($s -match 'GORAZDE|FOCA|CAJNICE|RUDO|ROGATICA|USTIKOLINA') { return 'Goražde' }
    if ($s -match 'BANJA_LUKA|GRADISKA|PRNJAVOR|CELINAC|STRICICI|MANJACA|NOVI_SEHER') { return 'Zenica' }


    
    if ($s -match 'SARAJEVO|ILIDZA|VOGOSCA|ALIPASINO|HRASNICA|KOBILJACA|GLADNO|DRAKSENIC|STUP|MISEVICI|HALILOVICI|DMALTA|OBALA|BASCARSIJA') { return 'Sarajevo' }
    if ($s -match 'TUZLA|GRAČANICA|LUKAVAC|KALESIJA|TISCA|KLJESTANI|BIJELJINA|ZVORNIK|SREBRENIK|TETIMA|JELASKE') { return 'Tuzla' }
    if ($s -match 'ZENICA|KAKANJ|VISOKO|ZAVIDOVI|TEŠANJ|VAREŠ|PUHOVI|NEMILA') { return 'Zenica' }
    if ($s -match 'MOSTAR|ČAPLJINA|ŠIROKI|GRUDE|LJUBUŠKI|KONJIC|JABLANICA|POSUSJE|PAPRASKO') { return 'Mostar' }
    if ($s -match 'BIHAC|CAZIN|VELIKAKLADUSA|SANSKI|KLJUC|BOSANSKI|NOVI SEHER|OSTROZAC|IZACIC') { return 'Bihać' }
    if ($s -match 'TRAVNIK|DVAKUF|JAJCE|VITEZ|BUGOJNO|GORNJI_VAKUF|NOVI_TRAVNIK|KAKRINJE') { return 'Travnik' }
    if ($s -match 'GORAZDE|FOCA|CAJNICE|RUDO|ROGATICA|USTIKOLINA|JOSANICA') { return 'Goražde' }
    if ($s -match 'BANJA_LUKA|GRADISKA|PRNJAVOR|CELINAC|STRICICI|BUNAREVI|LJUBIC') { return 'Banja Luka' }

    return 'Ostalo'
}

function Get-DurationInInterval {
    param([System.Collections.ArrayList]$events, [DateTime]$startDate, [DateTime]$endDate)
    $grouped = $events | Where-Object { $_.System -ne "IgnitionSCADA" } | Group-Object { "$($_.System)|$($_.Site)|$($_.Alarm)" }
    $result = @()
    foreach ($grp in $grouped) {
        $sorted = $grp.Group | Sort-Object Time
        $totalDur = 0; $activeStart = $null; $lastStatus = "CLEARED"; $count = 0
        for ($i = 0; $i -lt $sorted.Count; $i++) {
            $e = $sorted[$i]
            if ($e.Time -lt $startDate -or $e.Time -gt $endDate) { continue }
            if ($e.Status -match 'ACTIVE|MAJOR|CRITICAL') {
                if ($null -eq $activeStart) { $activeStart = $e.Time; $count++; $lastStatus = "ACTIVE" }
            } elseif ($e.Status -match 'CLEARED|MINOR|NORMAL') {
                if ($activeStart) {
                    $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
                    $overlapEnd = if ($e.Time -lt $endDate) { $e.Time } else { $endDate }
                    if ($overlapEnd -gt $overlapStart) { $totalDur += ($overlapEnd - $overlapStart).TotalMinutes }
                    $activeStart = $null; $lastStatus = "CLEARED"
                }
            }
        }
        if ($activeStart) {
            $overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
            $totalDur += ($endDate - $overlapStart).TotalMinutes
        }
        if ($totalDur -gt 0 -or $count -gt 0) {
            $parts = $grp.Name -split '\|'
            $result += [PSCustomObject]@{
                System = $parts[0]; Site = $parts[1]; Alarm = $parts[2]
                Region = $sorted[0].Region
                DayCnt = $count; DayDur = [Math]::Round($totalDur, 1); LastStatus = $lastStatus
            }
        }
    }
    $ignEvents = $events | Where-Object { $_.System -eq "IgnitionSCADA" -and $_.Time -ge $startDate -and $_.Time -le $endDate }
    $ignGrouped = $ignEvents | Group-Object { "$($_.Site)|$($_.Alarm)" } | ForEach-Object {
        $parts = $_.Name -split '\|'
        [PSCustomObject]@{
            System = "IgnitionSCADA"; Site = $parts[0]; Alarm = $parts[1]
            Region = $_.Group[0].Region; DayCnt = $_.Count; DayDur = 0; LastStatus = "UNKNOWN"
        }
    }
    return $result + $ignGrouped
}

# =========================================================
# MAIN LOOP
# =========================================================
while ($true) {
    try {
        $now = Get-Date; $today = $now.Date; $weekAgo = $today.AddDays(-7)
        $allEvents = [System.Collections.ArrayList]::new()
        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..." -ForegroundColor Cyan


       # === UNIVERZALNI PARSER ZA SVE SISTEME ===
		function Parse-AlarmLine {
			param([string]$Line)
			
			if ([string]::IsNullOrWhiteSpace($Line) -or $Line -notlike "*,*") { return $null }
			
			$parts = $Line.Split(',').ForEach({ $_.Trim().Trim('"') }) | Where-Object { $_ }
			if ($parts.Count -lt 4) { return $null }
			
			$alarm = [PSCustomObject]@{
				System = ''; Site = ''; Alarm = ''; Status = ''; Time = $null; Region = 'N/A'; IP = ''
			}
			
			# === IGNITION SCADA ===
			if ($parts[0] -eq 'IgnitionSCADA') {
				$alarm.System = 'IgnitionSCADA'
				# Site je u formatu "Sarajevo - Alipasino Polje" → izvući Region i Site
				$fullSite = $parts[1]
				if ($fullSite -match '^([A-Za-z]+)\s*-\s*(.+)$') {
					$alarm.Region = $Matches[1].Trim()
					$alarm.Site = $Matches[2].Trim().ToUpper().Replace(' ', '_')
				} else {
					$alarm.Site = $fullSite.ToUpper().Replace(' ', '_')
					$alarm.Region = Get-RegionFromSite $alarm.Site
				}
				$alarm.Alarm = $parts[2]
				$alarm.Status = $parts[4].Trim().ToUpper()  # cleared, critical, UNKNOWN
				$ts = $parts[5].Trim() -replace '_', ' '
				$alarm.Time = ConvertTo-DateTime $ts
			}
			
			# === NETECO ===
			elseif ($parts[0] -eq 'NetEco') {
				$alarm.System = 'NetEco'
				$alarm.Site = $parts[1].Trim().ToUpper()
				$alarm.Alarm = $parts[2].Trim()
				$ts = $parts[3].Trim()
				$alarm.Time = ConvertTo-DateTime $ts
				$alarm.Status = $parts[4].Trim().ToUpper()  # cleared, major, critical
				$alarm.Region = Get-RegionFromSite $alarm.Site
			}
			
			# === RPS-SC200/300, DSE-74xx, BARAN, EATON ===
			elseif ($parts[0] -match 'RpsSc300Mib|RPS-SC200-MIB|DSE-74xx|BARAN|EATON|RITTAL') {
				$alarm.System = $parts[0].Trim()
				$alarm.Site = $parts[1].Trim().ToUpper()
				# === KLJUČNO: Kolona 3 je Direkcija/Region! ===
				$alarm.Region = $parts[2].Trim()
				$alarm.Alarm = $parts[3].Trim() -replace '_', ' '
				$alarm.Status = $parts[-1].Trim().ToUpper()  # Zadnji element = status
				$ts = $parts[5].Trim() -replace '_', ' '
				$alarm.Time = ConvertTo-DateTime $ts
				if ($parts.Count -gt 7) { $alarm.IP = $parts[6].Trim() }
			}
			
			# === U2020 ===
			elseif ($parts[0] -eq 'U2020') {
				$alarm.System = 'U2020'
				$alarm.Site = $parts[1].Trim().ToUpper()
				$alarm.Alarm = $parts[2].Trim()
				$ts = $parts[3].Trim()
				$alarm.Time = ConvertTo-DateTime $ts
				$alarm.Status = $parts[4].Trim().ToUpper()  # cleared, major, minor
				$alarm.Region = Get-RegionFromSite $alarm.Site
				if ($parts.Count -gt 6) { $alarm.Hub = $parts[6].Trim() }
			}
			
			# Preskoči ako nema validan timestamp
			if ($null -eq $alarm.Time) { return $null }
			
			return $alarm
		}

        Write-Host "  UKUPNO: $($allEvents.Count) dogadjaja"
		
        # --- Agregacija ---
		function Get-DurationInInterval {
			param([System.Collections.ArrayList]$events, [DateTime]$startDate, [DateTime]$endDate)
			
			# Grupiši po System+Site+Alarm (ključ za korelaciju)
			$grouped = $events | Group-Object { "$($_.System)|$($_.Site)|$($_.Alarm)" }
			$result = @()
			
			foreach ($grp in $grouped) {
				$sorted = $grp.Group | Sort-Object Time
				$totalDur = 0; $activeStart = $null; $lastStatus = "CLEARED"; $count = 0
				
				foreach ($e in $sorted) {
					if ($e.Time -lt $startDate -or $e.Time -gt $endDate) { continue }
					
					# Aktivni statusi: ACTIVE, MAJOR, CRITICAL
					if ($e.Status -match 'ACTIVE|MAJOR|CRITICAL') {
						if ($null -eq $activeStart) { 
							$activeStart = $e.Time
							$count++  # Broji samo prvo pojavljivanje u paru
						}
					}
					# Cleared statusi: CLEARED, MINOR, NORMAL
					elseif ($e.Status -match 'CLEARED|MINOR|NORMAL') {
						if ($activeStart) {
							# Izračunaj preklapanje sa intervalom
							$overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
							$overlapEnd = if ($e.Time -lt $endDate) { $e.Time } else { $endDate }
							
							if ($overlapEnd -gt $overlapStart) {
								$totalDur += ($overlapEnd - $overlapStart).TotalMinutes
							}
							$activeStart = $null
							$lastStatus = "CLEARED"
						}
					}
				}
				
				# Ako je alarm još uvijek ACTIVE na kraju intervala
				if ($activeStart) {
					$overlapStart = if ($activeStart -gt $startDate) { $activeStart } else { $startDate }
					$totalDur += ($endDate - $overlapStart).TotalMinutes
					$lastStatus = "ACTIVE"
				}
				
				if ($totalDur -gt 0 -or $count -gt 0) {
					$result += [PSCustomObject]@{
						System = $grp.Name.Split('|')[0]
						Site = $grp.Name.Split('|')[1]
						Alarm = $grp.Name.Split('|')[2]
						Region = $sorted[0].Region
						DayCnt = $count
						DayDur = [Math]::Round($totalDur, 1)
						LastStatus = $lastStatus
					}
				}
			}
			return $result
		}

        # === CSV PARSE ===
        $csvCount = 0
        try {
            $csvResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/ispadnap" -UseBasicParsing -TimeoutSec 30
            $csvLines = [System.Text.Encoding]::UTF8.GetString($csvResponse.Content) -split "`r?`n"
            foreach ($line in $csvLines) {
                if ([string]::IsNullOrWhiteSpace($line) -or $line -notlike "*,*") { continue }
                $parts = $line.Split(',') | ForEach-Object { $_.Trim().Trim('"') } | Where-Object { $_ }
                if ($parts.Count -lt 5) { continue }
                $tsIdx = -1
                for ($i = 0; $i -lt $parts.Count; $i++) {
                    if ($parts[$i] -match '\d{4}-\d{2}-\d{2}[ _]\d{2}:\d{2}:\d{2}') { $tsIdx = $i; break }
                }
                if ($tsIdx -eq -1) { continue }
                $system = $parts[0].Trim(); $siteRaw = $parts[1].Trim(); $alarm = $parts[2].Trim()
                $time = ConvertTo-DateTime ($parts[$tsIdx] -replace '_', ' ')
                if ($null -eq $time) { continue }
                $status = "ACTIVE"
                if ($system -eq "IgnitionSCADA") {
                    $status = if ($parts.Count -gt 4) { $parts[4].Trim().ToUpper() } else { "UNKNOWN" }
                } else {
                    $statusIdx = if ($tsIdx + 1 -lt $parts.Count) { $tsIdx + 1 } else { $parts.Count - 1 }
                    $status = $parts[$statusIdx].Trim().ToUpper()
                    if ($status -notmatch 'ACTIVE|CLEARED|MAJOR|MINOR|CRITICAL|NORMAL') { $status = "ACTIVE" }
                }
                $site = $siteRaw.ToUpper().Replace(" ", "_").Replace("-", "_")
                $region = if ($system -match 'RpsSc300Mib|RPS-SC200-MIB' -and $parts.Count -gt 2) {
                    $parts[2].Trim()
                } elseif ($system -eq "IgnitionSCADA" -and $siteRaw -match '^([A-Za-z]+)\s*-\s*(.+)$') {
                    $Matches[1].Trim()
                } else {
                    Get-RegionFromSite $site
                }
                $allEvents.Add([PSCustomObject]@{ System=$system; Site=$site; Alarm=$alarm; Time=$time; Status=$status; Region=$region }) | Out-Null
                $csvCount++
            }
            Write-Host " CSV: $csvCount događaja" -ForegroundColor Green
        } catch { Write-Host " CSV greška: $($_.Exception.Message)" -ForegroundColor Red }

        # === HTML PARSE ===
        $htmlCount = 0
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = [System.Text.Encoding]::UTF8.GetString($htmlResponse.Content)
            $lines = $htmlContent -split "`r?`n"; $currentSection = "NETWORK"
            foreach ($line in $lines) {
                if ([string]::IsNullOrWhiteSpace($line)) { continue }
                if ($line -match '^\s*-+\s*([A-Z]+)\s*-+\s*$') { $currentSection = $Matches[1].Trim(); continue }
                if ($line -match '\b(\d{1,2})\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(\d{4})\s+(\d{2}:\d{2}:\d{2})') {
                    $dateStr = "$($Matches[1]) $($Matches[2]) $($Matches[3]) $($Matches[4])"
                    $time = ConvertTo-DateTime $dateStr
                    if ($null -eq $time) { continue }
                    $idx = $line.IndexOf($dateStr)
                    if ($idx -gt 0) {
                        $siteRaw = $line.Substring(0, $idx).Trim() -replace '<[^>]+>', '' -replace '&nbsp;', ' ' -replace '\s+', ' '
                        if ($siteRaw -and $siteRaw -notmatch '^[-]+$') {
                            $siteNorm = $siteRaw.ToUpper().Replace(" ", "_").Replace("-", "_")
                            $allEvents.Add([PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE is Disconnected"; Time=$time; Status="ACTIVE"; Region=Get-RegionFromSite $siteNorm }) | Out-Null
                            $htmlCount++
                        }
                    }
                }
            }
            Write-Host " HTML: $htmlCount događaja" -ForegroundColor Green
        } catch { Write-Host " HTML greška: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host " UKUPNO: $($allEvents.Count) događaja" -ForegroundColor Yellow


        # === AGREGACIJA ===
        $dailyAgg = Get-DurationInInterval $allEvents $today $now
        $weeklyAgg = Get-DurationInInterval $allEvents $weekAgo $now
        $allStats = @{}
        foreach ($d in $dailyAgg) {
            $key = "$($d.Site)|$($d.Alarm)|$($d.System)"
            $allStats[$key] = [PSCustomObject]@{ Site=$d.Site; Alarm=$d.Alarm; System=$d.System; Region=$d.Region; DayCnt=$d.DayCnt; DayDur=$d.DayDur; WeekCnt=0; WeekDur=0; LastStatus=$d.LastStatus }
        }
        foreach ($w in $weeklyAgg) {
            $key = "$($w.Site)|$($w.Alarm)|$($w.System)"
            if ($allStats.ContainsKey($key)) { $allStats[$key].WeekCnt = $w.DayCnt; $allStats[$key].WeekDur = $w.DayDur }
            else { $allStats[$key] = [PSCustomObject]@{ Site=$w.Site; Alarm=$w.Alarm; System=$w.System; Region=$w.Region; DayCnt=0; DayDur=0; WeekCnt=$w.DayCnt; WeekDur=$w.DayDur; LastStatus=$w.LastStatus } }
        }

        $finalStats = $allStats.Values | ForEach-Object {
            [PSCustomObject]@{
                System = $_.System; Site = $_.Site; Alarm = $_.Alarm; Region = $_.Region; LastStatus = $_.LastStatus
                DayCnt = if ($_.DayCnt) { [int]$_.DayCnt } else { 0 }
                DayDur = if ($_.DayDur) { [double]$_.DayDur } else { 0.0 }
                WeekCnt = if ($_.WeekCnt) { [int]$_.WeekCnt } else { 0 }
                WeekDur = if ($_.WeekDur) { [double]$_.WeekDur } else { 0.0 }
                MonthCnt = 0; MonthDur = 0.0; YearCnt = 0; YearDur = 0.0
            }
        }

        # === OUTPUT ===
        $newAlarms = [Math]::Max(0, $allEvents.Count - $previousCount)
        $previousCount = $allEvents.Count; $newAlarms | Out-File $counterFile -Force
        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v31 - $($now.ToString('HH:mm:ss')) " -ForegroundColor White
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih događaja: $newAlarms"
        Write-Host "-" -ForegroundColor Gray
        Write-Host "TOP 20 DNEVNIH ISPADA NAPAJANJA:" -ForegroundColor Yellow
        $dailyAgg | Where-Object { $_.DayDur -gt 0 } | Sort-Object DayDur -Descending | Select-Object -First 20 | ForEach-Object { Write-Host " $($_.Site) - $($_.DayDur) min" }

        $output = @{
            LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
            Stats = $finalStats
            Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status, Region -First 500
        }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        # === GIT SYNC ===
        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        git add $statsFile 2>&1 | Out-Null
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        git pull --rebase --autostash 2>&1 | Out-Null
        $pushResult = git push 2>&1
        if ($LASTEXITCODE -ne 0) { Write-Host "Git push greška: $pushResult" -ForegroundColor Red } else { Write-Host "Push završen." -ForegroundColor Green }

    } catch {
        Write-Host "GLAVNA GREŠKA: $($_.Exception.Message)" -ForegroundColor Red
			}
	
	        # ==========================================
			# KREIRANJE I ČUVANJE OUTPUTA (STATS_DATA.JSON)
			# ==========================================
			
			# 1. Priprema objekta za izlaz
			$output = @{ 
				LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss")
				Stats      = $finalStats 
				Recent     = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500 
			}

			# 2. Konverzija u JSON (Depth 10 je sigurnije za kompleksne objekte)
			$jsonString = $output | ConvertTo-Json -Depth 10

			# 3. Čišćenje JSON-a od suvišnih razmaka u ključevima (Sigurnosna mjera)
			# Ovo rješava problem ako PowerShell doda razmake u imena polja
			$jsonString = $jsonString -replace '"(\w+)\s*"\s*:', '"$1":'

			# 4. Čuvanje fajla BEZ BOM-a (Bitno za web dashboard!)
			# utf8NoBOM osigurava da browser ispravno čita JSON bez čudnih karaktera na početku
			try {
				$jsonString | Out-File -FilePath $statsFile -Encoding utf8NoBOM -Force
				Write-Host "[$(Get-Date -Format 'HH:mm:ss')] stats_data.json uspješno ažuriran." -ForegroundColor Green
			} catch {
				Write-Host "[$(Get-Date -Format 'HH:mm:ss')] GREŠKA pri upisu fajla: $_" -ForegroundColor Red
			}

			# ==========================================
			# KRAJ CIKLUSA / PAUZA
			# ==========================================
			
		# Ako želiš da se skripta vrti u beskonačnoj petlji, ostavi ovo:
		Start-Sleep -Seconds 300 # Čekaj 5 minuta prije sljedećeg ciklusa
			
		} # Kraj while petlje (ako postoji)
	
    Start-Sleep -Seconds 60
}