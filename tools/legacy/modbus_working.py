# solar_to_json.py
# ArcGIS Pro Python 3.6.8 compatible (Anaconda)
# Requires: pymodbus 2.5.3
# Terminal output: CSV format (comma separator) - SAMO za Eaton type
#
# REFINED - fixes applied:
#	1. Alarm segment 1201-1270 -> 1201-1272 (missed Multiple-Solar-Comms-Lost, Unstable-Rectifier-AC)
#	2. robust_read_discrete_inputs / robust_read_coils retries default 1 -> MAX_RETRIES
#	3. read_discrete_inputs_segmented delay hardcode 0.02 -> SLEEP_BETWEEN_REQUESTS
#	4. bits list() guard before .extend() in read_discrete_inputs_segmented
#	5. save_json makedirs guard for empty dirname
#	6. Removed dead code: safe_read_bits, safe_read_coils (superseded by robust_* variants)
#	7. CSV header fires per poll cycle, not per device entry (_csv_poll_cycle)

from __future__ import print_function
import os
import sys
import json
import time
import math
import struct
import hashlib
import socket
import logging
import threading
from datetime import datetime, timezone, timedelta
from collections import deque, defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed

try:
	from pymodbus.client.sync import ModbusTcpClient
except Exception as e:
	raise SystemExit('pymodbus.sync not available: %s' % e)

# ---------------- CONFIG ----------------
BASE_FOLDER = r'C:/Users/rusmirs/OneDrive - BH Telecom d.d. Sarajevo, BIH/Solar Data'
JSON_FILE = os.path.join(BASE_FOLDER, 'solar_data.json')
PREVIOUS_DATA_FILE = os.path.join(BASE_FOLDER, 'previous_data.json')
AVG_DATA_FILE = os.path.join(BASE_FOLDER, 'avg_data.json')
PLCS_FILE = os.path.join(BASE_FOLDER, 'plcs.json')
ALARM_MAP_FILE = os.path.join(BASE_FOLDER, 'alarmna_lista.json')

READ_INTERVAL = 120
SLEEP_BETWEEN_REQUESTS = 0.04
DISCRETE_BLOCK_SIZE = 8
TIMEOUT = 3.5
MAX_RETRIES = 2
log_path = os.path.join(BASE_FOLDER, 'modbus_solar.log')
logging.basicConfig(filename=log_path, level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')

# --------------- CSV Configuration ----------------
CSV_COLUMNS = [
	'Timestamp', 'Datum', 'PLCIP', 'FNE', 'Status', 'Alarmi',
	'P-solar (kW)', 'P-load (kW)', 'U-battery (V)', 'AC-Voltage (V)',
	'E-ukupno (kWh)', 'E-dnevno (kWh)', 'T-dnevno (h)',
	'E-load (kWh)', 'P-generator (kW)', 'E-generator (kWh)',
	'E-battery (kWh)', 'Nivogoriva (l)', 'Serial_number', 'SW_Version'
]
_csv_header_printed = False
_csv_poll_cycle = 0	 # FIX-7: incremented once per full poll cycle in sampler_loop

# --------------- Utilities ----------------
def now_iso_utc():
	return datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')

def to_date_local(ts=None):
	dt = ts or datetime.now(timezone.utc)
	return dt.astimezone(timezone(timedelta(hours=2))).strftime('%Y-%m-%d')

def check_network(ip, port, timeout=2):
	try:
		sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
		sock.settimeout(timeout)
		try:
			res = sock.connect_ex((ip, port))
		finally:
			sock.close()
		return res == 0
	except Exception:
		return False

def load_json_if_exists(path, default):
	try:
		if os.path.exists(path):
			with open(path, 'r', encoding='utf-8') as f:
				return json.load(f)
	except Exception as e:
		logging.warning(f'Failed to load json {path}: {e}')
	return default

def save_json(path, obj):
	try:
		# FIX-5: guard against empty dirname (bare filename without directory)
		dirname = os.path.dirname(path)
		if dirname:
			os.makedirs(dirname, exist_ok=True)
		with open(path, 'w', encoding='utf-8') as f:
			json.dump(obj, f, indent=2, ensure_ascii=False)
		logging.debug(f'Saved JSON to {path}')
	except Exception as e:
		logging.error(f'Failed to save json {path}: {e}')

def modbus_addr(doc_addr, base0):
	return doc_addr - 1 if base0 else doc_addr

# --------------- CSV Output (Eaton Only) ----------------
def print_csv_header():
	"""Ispisuje CSV header sa zarezom kao separatorom"""
	print(",".join(CSV_COLUMNS), flush=True)

def format_csv_value(val):
	"""Formatira vrijednost za CSV - handle None, strings, numbers"""
	if val is None:
		return "N/A"
	if isinstance(val, str):
		# Ako string sadrži zarez ili navodnike, escapuj ga
		if ',' in val or '"' in val or '\n' in val:
			escaped = val.replace('"', '""')
			return f'"{escaped}"'
		return val
	return str(val)

def print_eaton_csv(entry):
	"""Ispisuje Eaton entry kao CSV red sa zarezom separatorom"""
	global _csv_header_printed
	# FIX-7: header on first entry, then refresh every 5 full poll cycles (~10 min)
	if not _csv_header_printed or _csv_poll_cycle % 5 == 0:
		print_csv_header()
		_csv_header_printed = True
	row = [format_csv_value(entry.get(col)) for col in CSV_COLUMNS]
	print(",".join(row), flush=True)

# ---------------- In-memory state ----------------
_samples = defaultdict(lambda: defaultdict(deque))
_sampler_stop = threading.Event()
_prev_lock = threading.Lock()
_samples_lock = threading.Lock()
previous_data = load_json_if_exists(PREVIOUS_DATA_FILE, {})
if not isinstance(previous_data, dict):
	logging.warning("previous_data loaded is not a dict, resetting to {}")
	previous_data = {}
avg_loaded = load_json_if_exists(AVG_DATA_FILE, {})
if not isinstance(avg_loaded, dict):
	logging.warning("avg_data loaded is not a dict, resetting to {}")
	avg_loaded = {}
samples_per_period = math.ceil((15 * 60) / READ_INTERVAL)
for ip, metrics in avg_loaded.items():
	for m, lst in metrics.items():
		_samples[ip][m] = deque(lst, maxlen=samples_per_period)

# --------------- PLC & Alarm ----------------
def add_new_plc():
	print("\n=== Dodavanje novog PLC-a ===")
	while True:
		ip = input("Unesi PLC IP adresu: ").strip()
		if not check_network(ip, 502):
			print(f"Neuspješna konekcija na {ip}:502. Pokušaj ponovo.")
			continue
		break
	port = input("Unesi Modbus TCP port (default 502): ").strip() or '502'
	unit = int(input("Unesi Modbus Unit ID: ").strip())
	name_fallback = input("Unesi ime lokacije: ").strip()
	dev_type = input("Unesi tip (eaton/smartlogger): ").strip().lower()
	base0 = input("Koristi base0 (True/False): ").strip().lower() == 'true'
	fne = input("FNE lokacija (True/False): ").strip().lower() == 'true'
	debug = input("Debug mode (True/False): ").strip().lower() == 'true'

	new_plc = {"ip": ip, "port": int(port), "unit": unit, "name_fallback": name_fallback, "type": dev_type, "base0": base0, "fne": fne, "debug": debug}
	plcs = load_json_if_exists(PLCS_FILE, [])
	plcs.append(new_plc)
	save_json(PLCS_FILE, plcs)
	print(f"✅ Dodano: {name_fallback} ({ip})")

def load_plcs():
	return load_json_if_exists(PLCS_FILE, [])

def load_alarm_map():
	raw = load_json_if_exists(ALARM_MAP_FILE, {})
	norm = {
		"eaton": {},
		"eaton_coils": {},
		"smartlogger": {"register_alarms": {}, "inverter_alarms": {}}
	}

	for k, v in raw.get("eaton", {}).items():
		try:
			norm["eaton"][int(k)] = v
		except:
			continue

	for k, v in raw.get("eaton_coils", {}).items():
		try:
			norm["eaton_coils"][int(k)] = v
		except:
			continue

	for reg_str, bits in raw.get("smartlogger", {}).items():
		try:
			reg = int(reg_str)
			norm["smartlogger"]["register_alarms"][reg] = {int(b): name for b, name in bits.items()}
		except:
			continue

	for alarm_id_str, alarm_name in raw.get("smartlogger_inverter", {}).items():
		try:
			norm["smartlogger"]["inverter_alarms"][int(alarm_id_str)] = alarm_name
		except:
			continue

	return norm

PLCS = load_plcs()
ALARM_MAP = load_alarm_map()

# --------------- Modbus helpers ----------------
def safe_read_registers(func, addr, count, unit, retries=MAX_RETRIES):
	for attempt in range(retries):
		try:
			rr = func(addr, count, unit=unit)
			if rr and not rr.isError() and hasattr(rr, 'registers'):
				return rr.registers
		except Exception as e:
			if attempt == retries - 1:
				logging.debug(f"Error reading registers at {addr}: {e}")
			time.sleep(SLEEP_BETWEEN_REQUESTS)
	return None

# FIX-2: default retries=MAX_RETRIES (was 1, inconsistent with safe_read_registers)
def robust_read_discrete_inputs(client, addr, count, unit, retries=MAX_RETRIES):
	for attempt in range(retries):
		try:
			result = client.read_discrete_inputs(addr, count, unit=unit)
			if hasattr(result, 'isError') and result.isError():
				exception_code = getattr(result, 'exception_code', 'unknown')
				logging.debug(f'Modbus exception reading discrete inputs at {addr}: {exception_code}')
				return None
			if result and hasattr(result, 'bits') and result.bits is not None:
				return result.bits
		except Exception as e:
			logging.debug(f'Error reading discrete inputs at {addr}: {str(e)}')
			if attempt < retries - 1:
				time.sleep(SLEEP_BETWEEN_REQUESTS)
	return None

# FIX-2: default retries=MAX_RETRIES (was 1, inconsistent with safe_read_registers)
def robust_read_coils(client, addr, count, unit, retries=MAX_RETRIES):
	for attempt in range(retries):
		try:
			result = client.read_coils(addr, count, unit=unit)
			if hasattr(result, 'isError') and result.isError():
				exception_code = getattr(result, 'exception_code', 'unknown')
				logging.debug(f'Modbus exception reading coils at {addr}: {exception_code}')
				return None
			if result and hasattr(result, 'bits') and result.bits is not None:
				return result.bits
		except Exception as e:
			logging.debug(f'Error reading coils at {addr}: {str(e)}')
			if attempt < retries - 1:
				time.sleep(SLEEP_BETWEEN_REQUESTS)
	return None

def read_float_from_regs(regs):
	try:
		packed = struct.pack('>HH', int(regs[0]) & 0xFFFF, int(regs[1]) & 0xFFFF)
		val = struct.unpack('>f', packed)[0]
		return None if math.isnan(val) else val
	except:
		return None

def read_u32_from_regs(regs):
	try:
		return (int(regs[0]) << 16) | int(regs[1])
	except Exception:
		return None

def read_i32_from_regs(regs):
	try:
		value = (int(regs[0]) << 16) | int(regs[1])
		if value & 0x80000000:
			value -= 0x100000000
		return value
	except Exception:
		return None

def eaton_status_from_summary_bits(bits):
	if not bits or len(bits) < 4:
		return 'Comm_error'
	if bits[0]:
		return 'Critical'
	if bits[1]:
		return 'Major'
	if bits[2]:
		return 'Minor'
	if bits[3]:
		return 'Warning'
	return 'OK'

# FIX-3: delay default -> SLEEP_BETWEEN_REQUESTS (was hardcoded 0.02, half of global)
# FIX-4: bits = list(bits) guard before .extend() to avoid mutating pymodbus response
def read_discrete_inputs_segmented(client, start_doc, end_doc, unit, base0, block_size=8, delay=SLEEP_BETWEEN_REQUESTS):
	total_bits = []
	addr = start_doc
	while addr <= end_doc:
		count = min(block_size, end_doc - addr + 1)
		a = modbus_addr(addr, base0)
		bits = robust_read_discrete_inputs(client, a, count, unit)
		if bits is None:
			total_bits.extend([0] * count)
			logging.debug(f'Failed to read discrete inputs at {addr}, count {count}')
		else:
			bits = list(bits)  # FIX-4: copy before mutation
			if len(bits) < count:
				bits.extend([0] * (count - len(bits)))
			total_bits.extend(bits[:count])
		addr += count
		if addr <= end_doc:
			time.sleep(delay)
	return total_bits

def sample_device(plc):
	ip = plc.get('ip')
	port = plc.get('port', 502)
	unit = plc.get('unit', 1)
	dev_type = plc.get('type', 'eaton')
	base0 = plc.get('base0', False)
	fne = plc.get('fne', False)
	location_name = plc.get('name_fallback', 'Unknown').lower()

	if not check_network(ip, port):
		return ip, None

	client = ModbusTcpClient(ip, port=port, timeout=TIMEOUT)
	ret = {'Status': 'Unknown', 'Alarmi': 'No alarms', 'IsLatest': True}

	try:
		if not client.connect():
			return ip, None

		if dev_type == 'smartlogger':
			registers_to_read = [
				(40567, 1, 'status'), (40525, 2, 'p_solar'), (40560, 2, 'e_ukupno'),
				(40562, 2, 'e_dnevno'), (40564, 2, 't_dnevno'), (50001, 1, 'alarms')
			]
			results = {}
			for addr, count, key in registers_to_read:
				regs = safe_read_registers(client.read_holding_registers, modbus_addr(addr, base0), count, unit)
				if regs:
					results[key] = regs

			if 'status' in results:
				val = int(results['status'][0])
				status_map = {1: 'On-grid', 2: 'Outage', 3: 'Maintenance', 4: 'Idle'}
				ret['Status'] = status_map.get(val, 'Unknown')
			if 'p_solar' in results:
				v = read_i32_from_regs(results['p_solar'])
				if v is not None:
					ret['P-solar (kW)'] = round(float(v) / 1000.0, 2)
					with _samples_lock:
						_samples[ip]['P-solar (kW)'].append(ret['P-solar (kW)'])
			if 'e_ukupno' in results:
				v = read_u32_from_regs(results['e_ukupno'])
				ret['E-ukupno (kWh)'] = round(float(v) / 10.0, 2) if v is not None else None
			if 'e_dnevno' in results:
				v = read_u32_from_regs(results['e_dnevno'])
				ret['E-dnevno (kWh)'] = round(float(v) / 10.0, 2) if v is not None else None
			if 't_dnevno' in results:
				v = read_u32_from_regs(results['t_dnevno'])
				ret['T-dnevno (h)'] = round(float(v) / 10.0, 2) if v is not None else None

			alarm_list = []
			if 'alarms' in results:
				bits_val = int(results['alarms'][0])
				for bit_pos in range(16):
					if bits_val & (1 << bit_pos):
						bit_number = bit_pos + 1
						alarm_name = ALARM_MAP['smartlogger'].get('register_alarms', {}).get(50001, {}).get(bit_number)
						if alarm_name:
							alarm_list.append(alarm_name)
			ret['Alarmi'] = '; '.join(alarm_list) if alarm_list else 'No alarms'

		elif dev_type == 'eaton':
			start_addr = modbus_addr(1001, base0)
			status_bits = robust_read_discrete_inputs(client, start_addr, 4, unit)
			ret['Status'] = eaton_status_from_summary_bits(status_bits) if status_bits is not None else 'Unknown'

			alarms_list = []
			alarm_map = ALARM_MAP
			# FIX-1: segment end 1270 -> 1272 (map: 1271=Multiple-Solar-Comms-Lost, 1272=Unstable-Rectifier-AC)
			# 1273-1300 is illegal per SC300 map — boundary unchanged
			segments = [(1101, 1107), (1201, 1272), (1301, 1304)]
			for segment_start, segment_end in segments:
				segment_bits = read_discrete_inputs_segmented(
					client, segment_start, segment_end, unit, base0,
					block_size=DISCRETE_BLOCK_SIZE
				)
				if segment_bits:
					for idx, addr in enumerate(range(segment_start, segment_end + 1)):
						if addr in alarm_map.get('eaton', {}) and idx < len(segment_bits) and segment_bits[idx]:
							alarms_list.append(alarm_map['eaton'][addr])

			try:
				coil_start_addr = modbus_addr(1, base0)
				coil_bits = robust_read_coils(client, coil_start_addr, 6, unit)
				if coil_bits is not None:
					coil_alarm_map = alarm_map.get('eaton_coils', {})
					for coil_addr in range(1, 7):
						bit_pos = coil_addr - 1
						if bit_pos < len(coil_bits) and coil_bits[bit_pos]:
							alarm_name = coil_alarm_map.get(coil_addr)
							if alarm_name:
								alarms_list.append(alarm_name)
			except Exception as e:
				logging.debug(f'{ip} error reading coil alarms: {e}')

			ret['Alarmi'] = '; '.join(alarms_list) if alarms_list else 'No alarms'

			registers_to_read = [
				(7017, 2, 'ac_voltage'), (6001, 2, 'serial'), (7009, 2, 'p_load'),
				(7001, 2, 'u_battery'), (3001, 1, 'sw_major'), (3002, 1, 'sw_minor'), (3003, 1, 'sw_mv'),
			]
			if fne:
				registers_to_read.extend([(7317, 2, 'p_solar'), (7031, 2, 'e_ukupno_pri'), (7035, 2, 'e_load_pri')])
			if location_name == 'mliniste':
				registers_to_read.extend([(7015, 2, 'p_generator'), (7503, 2, 'e_generator'), (7037, 2, 'e_battery'), (7107, 2, 'nivogoriva')])

			results = {}
			for addr, count, key in registers_to_read:
				regs = safe_read_registers(client.read_input_registers, modbus_addr(addr, base0), count, unit)
				if regs:
					results[key] = regs

			if 'ac_voltage' in results:
				v = read_float_from_regs(results['ac_voltage'])
				if v is not None: ret['AC-Voltage (V)'] = round(v, 2)
			if 'serial' in results:
				v = read_i32_from_regs(results['serial'])
				if v is not None: ret['Serial_number'] = v
			if 'p_load' in results:
				v = read_float_from_regs(results['p_load'])
				if v is not None: ret['P-load (kW)'] = round(v, 2)
			if 'u_battery' in results:
				v = read_float_from_regs(results['u_battery'])
				if v is not None: ret['U-battery (V)'] = round(v, 2)

			if fne:
				if 'p_solar' in results:
					v = read_float_from_regs(results['p_solar'])
					if v is not None:
						ret['P-solar (kW)'] = round(v, 2)
						with _samples_lock:
							_samples[ip]['P-solar (kW)'].append(ret['P-solar (kW)'])
				e_ukupno = None
				if 'e_ukupno_pri' in results:
					e_ukupno = read_float_from_regs(results['e_ukupno_pri'])
				if e_ukupno is None:
					fallback = safe_read_registers(client.read_input_registers, modbus_addr(7501, base0), 2, unit)
					if fallback: e_ukupno = read_float_from_regs(fallback)
				if e_ukupno is not None:
					prev_val = previous_data.get(ip, {}).get('E-ukupno (kWh)')
					if prev_val is not None and e_ukupno < prev_val: e_ukupno = prev_val
					ret['E-ukupno (kWh)'] = round(e_ukupno, 2)
				e_load = None
				if 'e_load_pri' in results:
					e_load = read_float_from_regs(results['e_load_pri'])
				if e_load is None:
					fallback = safe_read_registers(client.read_input_registers, modbus_addr(7505, base0), 2, unit)
					if fallback: e_load = read_float_from_regs(fallback)
				ret['E-load (kWh)'] = round(e_load, 2) if e_load is not None else None

				sarajevo_tz = timezone(timedelta(hours=2))
				now = datetime.now(sarajevo_tz)
				today = now.date()
				with _prev_lock:
					previous_data.setdefault(ip, {})
					pdata = previous_data[ip]
					if 'last_reset_date' not in pdata or pdata['last_reset_date'] != today:
						if 'E-ukupno (kWh)' in ret and ret['E-ukupno (kWh)'] is not None:
							pdata['E-ukupno_start_day'] = ret['E-ukupno (kWh)']
						pdata['T-dnevno_accum'] = 0.0
						pdata['last_reset_date'] = today
					if 'E-ukupno (kWh)' in ret and ret['E-ukupno (kWh)'] is not None:
						e_dnevno = ret['E-ukupno (kWh)'] - pdata.get('E-ukupno_start_day', ret['E-ukupno (kWh)'])
						ret['E-dnevno (kWh)'] = round(max(0, e_dnevno), 2)
					p_solar = ret.get('P-solar (kW)')
					if p_solar is not None and p_solar > 0:
						pdata['T-dnevno_accum'] = pdata.get('T-dnevno_accum', 0.0) + (READ_INTERVAL / 3600.0)
					ret['T-dnevno (h)'] = round(pdata.get('T-dnevno_accum', 0.0), 2)

			if location_name == 'mliniste':
				for reg_key, label in [('p_generator', 'P-generator (kW)'), ('e_generator', 'E-generator (kWh)'),
										('e_battery', 'E-battery (kWh)'), ('nivogoriva', 'Nivogoriva (l)')]:
					if reg_key in results:
						v = read_float_from_regs(results[reg_key])
						if v is not None: ret[label] = round(v, 2)

			if all(k in results for k in ['sw_major', 'sw_minor', 'sw_mv']):
				try:
					ret['SW_Version'] = f"{results['sw_major'][0]}.{results['sw_minor'][0]} modbus {results['sw_mv'][0]}"
				except:
					ret['SW_Version'] = 'N/A'

	except Exception as e:
		logging.debug(f'{ip} sampling error: {str(e)}')
		return ip, None
	finally:
		try:
			client.close()
		except:
			pass

	ret.update({
		'PLCIP': ip, 'Type': dev_type, 'FNE': plc.get('name_fallback', 'Unknown'),
		'Timestamp': now_iso_utc(), 'Datum': to_date_local()
	})
	return ip, ret

# ---------------- Sampling loop ----------------
def sampler_loop():
	global PLCS, ALARM_MAP, previous_data, _csv_header_printed, _csv_poll_cycle
	PLCS = load_plcs()
	ALARM_MAP = load_alarm_map()
	_csv_header_printed = False
	_csv_poll_cycle = 0	 # FIX-7: reset on restart

	max_workers = min(8, max(4, len(PLCS) // 2))
	logging.info(f'Sampler started: interval={READ_INTERVAL}s, workers={max_workers}')

	with ThreadPoolExecutor(max_workers=max_workers) as ex:
		while not _sampler_stop.is_set():
			start = time.time()
			futures = {ex.submit(sample_device, plc): plc for plc in PLCS}
			all_entries = {}
			ts = now_iso_utc()

			for fut in as_completed(futures):
				plc = futures[fut]
				ip = plc['ip']
				try:
					_, data = fut.result(timeout=10)
				except Exception as e:
					data = None
					logging.debug(f'Sampling failed for {ip}: {e}')

				fne = plc.get('name_fallback', 'Unknown')
				entry_base = {
					'Timestamp': ts, 'Datum': to_date_local(), 'FNE': fne, 'PLCIP': ip,
					'Type': plc.get('type', 'eaton'),
					'unique_id': hashlib.md5((ip + fne).encode('utf-8')).hexdigest(),
				}

				if data is not None:
					entry = entry_base.copy()
					entry.update(data)
					entry['Status'] = data.get('Status', 'Unknown')
					entry['IsLatest'] = True
					with _prev_lock:
						previous_data[ip] = entry
					# CSV OUTPUT SAMO ZA EATON
					if plc.get('type') in ('eaton', 'smartlogger'):
						print_eaton_csv(entry)
				else:
					entry = entry_base.copy()
					entry['Status'] = 'Comm_error'
					entry['Alarmi'] = 'No alarms'
					entry['IsLatest'] = True
					with _prev_lock:
						previous_data[ip] = entry
				all_entries[ip] = entry

			if all_entries:
				solar_data = load_json_if_exists(JSON_FILE, [])
				if not isinstance(solar_data, list): solar_data = []
				ips_to_update = set(all_entries.keys())
				solar_data = [e for e in solar_data if e.get('PLCIP') not in ips_to_update]
				for ip, entry in all_entries.items():
					solar_data.append(entry)
				save_json(JSON_FILE, solar_data)
				save_json(PREVIOUS_DATA_FILE, previous_data)

			avg_dict = {}
			for ip, metrics in _samples.items():
				avg_dict[ip] = {}
				for m, dq in metrics.items():
					avg_dict[ip][m] = list(dq)
			save_json(AVG_DATA_FILE, avg_dict)

			# FIX-7: increment once per full poll cycle, after all devices done
			_csv_poll_cycle += 1

			elapsed = time.time() - start
			to_sleep = READ_INTERVAL - elapsed
			if to_sleep > 0:
				_sampler_stop.wait(to_sleep)

# ---------------- Main ----------------
if __name__ == '__main__':
	print("=== Solar to JSON - CSV Terminal Output ===")
	print("💡 Format: CSV sa zarezom (,) | Samo Eaton uređaji")
	print("💡 Kopiraj output direktno u .csv fajl ili Excel\n")
	choice = input("Pokreni skriptu ili dodaj novi PLC? (pokreni/dodaj): ").strip().lower()
	if choice == 'dodaj':
		add_new_plc()
		exit()
	try:
		sampler_thread = threading.Thread(target=sampler_loop, daemon=True)
		sampler_thread.start()
		while True:
			time.sleep(1)
	except KeyboardInterrupt:
		print("\n⏹ Stopping sampler...")
		_sampler_stop.set()
		sampler_thread.join()
		print("✅ Skripta zaustavljena.")