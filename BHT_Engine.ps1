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
# === DODAJ OVO NAKON ConvertTo-DateTime FUNKCIJE ===
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
    if ($s -match '_TUZLA$|^TUZLA_') { return 'Tuzla' }
    if ($s -match '_SARAJEVO$|^SARAJEVO_') { return 'Sarajevo' }
    if ($s -match '_ZENICA$|^ZENICA_') { return 'Zenica' }
    if ($s -match '_MOSTAR$|^MOSTAR_') { return 'Mostar' }
    if ($s -match '_BIHAC$|^BIHAC_') { return 'Bihać' }
    if ($s -match '_TRAVNIK$|^TRAVNIK_') { return 'Travnik' }
    if ($s -match '_GORAZDE$|^GORAZDE_') { return 'Goražde' }

    # 4. Fuzzy Match
    if ($s -match 'SARAJEVO|ILIDZA|VOGOSCA|ALIPASINO|HRASNICA|KOBILJACA|GLADNO|STUP|MISEVICI|HALILOVICI') { return 'Sarajevo' }
    if ($s -match 'TUZLA|GRAČANICA|LUKAVAC|KALESIJA|TISCA|KLJESTANI|BIJELJINA|ZVORNIK|SREBRENIK') { return 'Tuzla' }
    if ($s -match 'ZENICA|KAKANJ|VISOKO|ZAVIDOVICI|TEŠANJ|VAREŠ|BREZA|OLOVO|ŽEPČE|ZEPCE|STUPARI') { return 'Zenica' }
    if ($s -match 'MOSTAR|ČAPLJINA|ŠIROKI|GRUDE|LJUBUŠKI|KONJIC|JABLANICA|POSUSJE|PROZOR') { return 'Mostar' }
    if ($s -match 'BIHAC|CAZIN|VELIKA_KLADUSA|SANSKI_MOST|KLJUC|BOSANSKI_NOVI|DRAKSENIC|BUNAREVI') { return 'Bihać' }
    if ($s -match 'TRAVNIK|DVAKUF|JAJCE|VITEZ|BUGOJNO|GORNJI_VAKUF|NOVI_TRAVNIK') { return 'Travnik' }
    if ($s -match 'GORAZDE|FOCA|CAJNICE|RUDO|ROGATICA|USTIKOLINA') { return 'Goražde' }
    if ($s -match 'BANJA_LUKA|GRADISKA|PRNJAVOR|CELINAC|STRICICI|MANJACA|NOVI_SEHER') { return 'Zenica' }

    return 'Ostalo'
}
	# === KRAJ FUNKCIJE ===
while ($true) {
    try {
        $now = Get-Date
        $today = $now.Date
        $weekAgo = $today.AddDays(-7)
        $allEvents = @()
        Write-Host "[$($now.ToString('HH:mm:ss'))] Dohvatanje podataka..."

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

		# === Site Correlation (Priprema za buduÄ‡e mapiranje) ===
		$siteGroups = $allEvents.Site | Select-Object -Unique | ForEach-Object {
			$core = $_ -replace '_.*$', '' -replace '[-\s].*$', ''
			[PSCustomObject]@{ Original = $_; Core = $core }
		} | Group-Object Core
		# TODO: Kasnije dodati: $siteGroups | Export-Csv "site_mapping.csv" -NoTypeInformation
		# === KRAJ ===

		$dailyAgg  = Get-DurationInInterval $allEvents $today $now
		$weeklyAgg = Get-DurationInInterval $allEvents $weekAgo $now

		$allStats = @{}
		foreach ($d in $dailyAgg) {
			$key = "$($d.Site)|$($d.Alarm)|$($d.System)"
			$allStats[$key] = @{
				Site=$d.Site; Alarm=$d.Alarm; System=$d.System
				DayCnt=$d.Count; DayDur=$d.Duration
				WeekCnt=0; WeekDur=0
				LastStatus=$d.LastStatus
			}
		}
		foreach ($w in $weeklyAgg) {
			$key = "$($w.Site)|$($w.Alarm)|$($w.System)"
			if ($allStats.ContainsKey($key)) {
				$allStats[$key].WeekCnt = $w.Count
				$allStats[$key].WeekDur = $w.Duration
			} else {
				$allStats[$key] = @{
					Site=$w.Site; Alarm=$w.Alarm; System=$w.System
					DayCnt=0; DayDur=0
					WeekCnt=$w.Count; WeekDur=$w.Duration
					LastStatus=$w.LastStatus
				}
			}
		}

		# --- Kreiranje finalnih statistika sa region mapiranjem ---
		$finalStats = $allStats.Values | ForEach-Object {
			$region = Get-RegionFromSite $_.Site
			[PSCustomObject]@{
				System     = $_.System
				Site       = $_.Site
				Alarm      = $_.Alarm
				Region     = $region
				LastStatus = $_.LastStatus
				DayCnt     = [int]($_.DayCnt -or 0)
				DayDur     = [double]($_.DayDur -or 0)
				WeekCnt    = [int]($_.WeekCnt -or 0)
				WeekDur    = [double]($_.WeekDur -or 0)
				MonthCnt   = 0
				MonthDur   = 0
				YearCnt    = 0
				YearDur    = 0
			}
		}
		function Get-SiteCoreName {
			param([string]$RawSite)
			$s = $RawSite.Trim().ToUpper()
			
			# Ukloni prefikse za kros-validaciju
			$s = $s -replace '^BTS_', '' -replace '^C\d+?_', '' -replace '^NOKIA\d+?_', ''
			$s = $s -replace '^RRST_', '' -replace '^BS ', '' -replace ' - NEW$', ''
			
			# Za RR linkove "BS A-BS B" â†’ vrati oba sajta
			if ($s -match 'BS\s+([A-Z0-9_]+)\s*-\s*BS\s+([A-Z0-9_]+)') {
				return @($matches[1].Trim(), $matches[2].Trim())
			}
			
			return $s.Trim()
		}
		
		# Za HTML alarme (BTS, MPLS, RR):
		$alarmRows | ForEach-Object {
			$rawSite = ($_ -split '</td>')[0] -replace '<[^>]+>', '' -replace '^\s*<td>\s*', ''
			$coreSites = Get-SiteCoreName $rawSite
			
			# Ako je array (RR link), procesiraj oba sajta
			if ($coreSites -is [array]) {
				foreach ($core in $coreSites) {
					$region = Get-RegionFromSite $core
					# ... dodaj u $allEvents
				}
			} else {
				$region = Get-RegionFromSite $coreSites
				# ... dodaj u $allEvents
			}
		}
		
        $newAlarms = $allEvents.Count - $previousCount
        if ($newAlarms -lt 0) { $newAlarms = 0 }
        $previousCount = $allEvents.Count
        $newAlarms | Out-File $counterFile -Force

        Clear-Host
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host " BHT ENGINE v31 - $($now.ToString('HH:mm:ss'))"
        Write-Host "============================================================" -ForegroundColor Gray
        Write-Host "Novih dogaÄ‘aja: $newAlarms"
        Write-Host "------------------------------------------------------------" -ForegroundColor Gray
        Write-Host "TOP 20 DNEVNIH ISPADA NAPAJANJA:"
        $dailyAgg | Where-Object { $_.Duration -gt 0 } | Sort-Object Duration -Descending | Select-Object -First 20 | ForEach-Object { Write-Host "  $($_.Site) - $($_.Duration) min" }

        $output = @{ LastUpdate = $now.ToString("yyyy-MM-dd HH:mm:ss"); Stats = $finalStats; Recent = $allEvents | Select-Object System, Site, Alarm, Time, Status -First 500 }
        $output | ConvertTo-Json -Depth 5 | Set-Content $statsFile -Encoding UTF8

        Write-Host "Sinhronizacija sa GitHub-om..." -ForegroundColor Cyan
        git add $statsFile
        git commit -m "Auto-update $($now.ToString('yyyy-MM-dd HH:mm:ss'))" 2>&1 | Out-Null
        git pull --rebase --autostash 2>&1 | Out-Null
        $pushResult = git push 2>&1
        if ($LASTEXITCODE -ne 0) { Write-Host "Git push greÅ¡ka: $pushResult" -ForegroundColor Red }
        else { Write-Host "Push zavrÅ¡en." -ForegroundColor Green }
    }
    catch { Write-Host "GLAVNA GREÅ KA: $($_.Exception.Message)" -ForegroundColor Red }
    Start-Sleep -Seconds 60
}
