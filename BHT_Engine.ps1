# BHT Alarm Engine v31 - FINAL WORKING
$mutex = New-Object System.Threading.Mutex($false, "Global\BHTEngineMutex")
if (-not $mutex.WaitOne(0)) { Write-Host "Skripta veÄ‡ radi." -ForegroundColor Red; exit }

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

    # 1. EKSPlicitna pravila (najviši prioritet)
    $explicitMap = @{
        'GRABOVICA'       = 'Mostar'
        'GRABOVICA_TUZLA' = 'Tuzla'
        'TUZLA_KISELJAK'  = 'Tuzla'
        'KISELJAK_CENTAR' = 'Sarajevo'
        'POSUSJE_OSREDAK' = 'Mostar'
        'POSUSJE_CENTAR'  = 'Mostar'
        'MANJACA'         = 'Banja Luka'
        'KMUR'            = 'Goražde'
		'TRAVNIK_SUMECE'  = 'Travnik'
    }
    if ($explicitMap.ContainsKey($s)) { return $explicitMap[$s] }

    # 2. Sufiks/Prefiks pravila
    if ($s -match '_TUZLA$|^TUZLA_') { return 'Tuzla' }
    if ($s -match '_SARAJEVO$|^SARAJEVO_') { return 'Sarajevo' }
    if ($s -match '_ZENICA$|^ZENICA_') { return 'Zenica' }
    if ($s -match '_MOSTAR$|^MOSTAR_') { return 'Mostar' }
    if ($s -match '_BIHAC$|^BIHAC_') { return 'Bihać' }
    if ($s -match '_TRAVNIK$|^TRAVNIK_') { return 'Travnik' }
    if ($s -match '_GORAZDE$|^GORAZDE_') { return 'Goražde' }

    # 3. Fuzzy match (samo ako 1 i 2 ne odgovaraju)
    if ($s -match 'SARAJEVO|ILIDZA|VOGOSCA|ALIPASINO|HRASNICA|KOBILJACA|MISEVICI|GLADNO|DRAKSENIC|STUP|HALILOVICI') { return 'Sarajevo' }
    if ($s -match 'TUZLA|GRAČANICA|LUKAVAC|KALESIJA|KLJESTANI|TISCA') { return 'Tuzla' }
    if ($s -match 'ZENICA|KAKANJ|VISOKO|ZAVIDOVI|PUHOVI') { return 'Zenica' }
    if ($s -match 'MOSTAR|ČAPLJINA|ŠIROKI|GRUDE|LJUBUŠKI|KONJIC|JABLANICA') { return 'Mostar' }
    if ($s -match 'BIHAC|CAZIN|VELIKAKLADUSA|SANSKI|KLJUC|BOSANSKI') { return 'Bihać' }
    if ($s -match 'TRAVNIK|NOVITRAVNIK|JAJCE|VITEZ|BUGOJNO|KAKRINJE') { return 'Travnik' }
    if ($s -match 'GORAZDE|FOCA|CAJNICE|RUDO|ROGATICA') { return 'Goražde' }
    if ($s -match 'BANJALUKA|GRADISKA|PRNJAVOR|CELINAC|STRICICI|BUNAREVI') { return 'Banja Luka' }

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
                # Filtriraj Å¡um alarme
				$exclude = @('Node Info','Info','Modbus','PLC','8000','3000','1000','OPC_STATUS','UsageNormal')
				if ($exclude -notcontains $alarm) {
					$allEvents += [PSCustomObject]@{ System=$system; Site=$site; Alarm=$alarm; Time=$time; Status=$status }
					$csvCount++
				}
            }
            Write-Host "  CSV: $csvCount dogaÄ‘aja"
        } catch { Write-Host "  CSV greÅ¡ka: $($_.Exception.Message)" -ForegroundColor Red }

        # --- HTML ---
        $htmlCount = 0
        try {
            $htmlResponse = Invoke-WebRequest -Uri "https://pokrivenost.bhtelecom.ba/alarmi/" -UseBasicParsing -TimeoutSec 30
            $htmlContent = $response  # $response je veÄ‡ string, ne konvertuj u bytes
            $alarmRows = $htmlContent -split '</tr>' | Where-Object { $_ -match '<td>BTS_|<td>C\d+_|<td>NOKIA|<td>ATN_|<td>ASR_|<td>DWDM_|<td>BS_|<td>RRST_|<td>NCS_|' }
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
                        # Filtriraj Å¡um alarme (isti princip) DODATI TREBA
						$exclude = @('Node Info','Info','Modbus','PLC','OPC_STATUS')
						if ($exclude -notcontains "NE is Disconnected") {  # ovaj specifiÄno zadrÅ¾i
							$allEvents += [PSCustomObject]@{ System=$currentSection; Site=$siteNorm; Alarm="NE is Disconnected"; Time=$time; Status="ACTIVE" }
							$htmlCount++
						}
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
            Write-Host "  HTML: $htmlCount dogaÄ‘aja"
        } catch { Write-Host "  HTML greÅ¡ka: $($_.Exception.Message)" -ForegroundColor Red }

        Write-Host "  UKUPNO: $($allEvents.Count) dogaÄ‘aja"
		
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
