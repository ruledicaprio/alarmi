#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""
snmp_log_poller.py -- tail SNMP trap log files and push events to bht-api.

Runs on 192.168.132.117 (SNMP log server, Python 2.4).
Pushes to http://192.168.108.88:8080/ingest/events (bht-api on Rocky 9).

Deploy on 192.168.132.117:
    cp snmp_log_poller.py /root/snmplogovi/poller.py
    chmod +x /root/snmplogovi/poller.py
    nohup python /root/snmplogovi/poller.py >> /root/snmplogovi/poller.log 2>&1 &

Usage:
    python poller.py              # daemon, polls every 60 s, skips history on first run
    python poller.py --once       # single pass then exit (testing)
    python poller.py --catchup    # ingest all history from file start then daemon
    python poller.py --once --catchup   # ingest all history then exit
"""

import datetime
import os
import re
import socket
import sys
import time
import urllib2

# ── Configuration ──────────────────────────────────────────────────────────────

API_URL          = "http://192.168.108.88:8080/ingest/events"
STATE_FILE       = "/tmp/.snmp_poller_state.json"
POLL_SECS        = 60
MAX_BATCH        = 500
UTC_OFFSET_HOURS = 2    # CEST = UTC+2

FILES = [
    ("/root/snmplogovi/sve_napajanjeran.log", "u2020_flat"),
    ("/root/snmplogovi/ljutoc.log",           "snmp_trap"),
]

socket.setdefaulttimeout(30)

# ── Logging ────────────────────────────────────────────────────────────────────

def _log(msg):
    ts = datetime.datetime.utcnow().strftime("%Y-%m-%d %H:%M:%S UTC")
    sys.stdout.write("[%s] %s\n" % (ts, msg))
    sys.stdout.flush()

# ── Minimal JSON codec (no json module in Python 2.4) ─────────────────────────

def _jstr(v):
    if isinstance(v, unicode):
        v = v.encode("utf-8")
    out = ['"']
    for c in v:
        o = ord(c)
        if   c == '"':  out.append('\\"')
        elif c == '\\': out.append('\\\\')
        elif c == '\n': out.append('\\n')
        elif c == '\r': out.append('\\r')
        elif c == '\t': out.append('\\t')
        elif o < 32:    out.append('\\u%04x' % o)
        else:           out.append(c)
    out.append('"')
    return "".join(out)

def _jdumps(obj):
    if obj is None:
        return "null"
    if isinstance(obj, bool):
        if obj: return "true"
        return "false"
    if isinstance(obj, (int, long)):
        return str(obj)
    if isinstance(obj, (str, unicode)):
        return _jstr(obj)
    if isinstance(obj, list):
        return "[" + ",".join([_jdumps(x) for x in obj]) + "]"
    if isinstance(obj, dict):
        parts = [_jstr(k) + ":" + _jdumps(v) for k, v in obj.items()]
        return "{" + ",".join(parts) + "}"
    return _jstr(str(obj))

def _jloads(s):
    # Used only to parse {"inserted": N} responses and the state file
    # (which contains only strings and ints — no booleans or null).
    return eval(s)

# ── State ──────────────────────────────────────────────────────────────────────

def _load_state():
    try:
        f = open(STATE_FILE)
        try:
            data = f.read()
        finally:
            f.close()
        return _jloads(data)
    except (IOError, OSError, SyntaxError, ValueError):
        return {}

def _save_state(state):
    f = open(STATE_FILE, "w")
    try:
        f.write(_jdumps(state))
    finally:
        f.close()

# ── Normalization -- mirrors crates/normalize/src/{classify,parse}.rs ──────────

_RULES_RAW = [
    ("NE_DISCONNECTED",   r"\bne is disconnected\b|\bdisconnected\b"),
    ("COMMS_LOST",        r"gubitak komunikacije|prisustvo komunikacije|comms[- ]?lost"
                          r"|communication|snmpv2-mib|nepoznat|node info"),
    ("GENSET_EVENT",      r"engine ?(start|stop)|generator ?(start|stop|enable|over|under)"
                          r"|notifengine|notiflevel|levelstatus|namedalarm|\bdse\b|\bdea[_ ]"
                          r"|nivo goriva|fuel|oilpressure"),
    ("MAINS_FAILURE",     r"nestanak 220|mains ?(failure|fail)|mains phase l[123]|mains\d*voltage"
                          r"|ac[_ ]?fail|acinputfault|ac input fault|input fault|ac phase l[123]"
                          r"|partial[- ]ac[- ]fail|phase[- ]?fail|phase l[123]|under ?voltage"
                          r"|nestanak (mreže|mreze)|undervoltage|ispad faze|ispad[_ ]?mreze"
                          r"|blackout|notifmains|mains ?return"),
    ("RECTIFIER_FAILURE", r"kvar ispravlja|rectifier (power )?(failure|fail)"
                          r"|rectifier[- ]fail|ispravlja"),
    ("RECTIFIER_COMMS",   r"rectifier[- ]comms[- ]lost"),
    ("SOLAR_FAULT",       r"solar[_ ]?fail|solar[- ]comms[- ]lost"),
    ("UPS_MODULE",        r"ups .*modula|ups .*module|ups[- ]?fail|alarmi modula"
                          r"|inverter (fault|fail)|ispad[_ ]?invertora|invertor|bypass"),
    ("HIGH_VOLTAGE",      r"visok napon|over[- ]?voltage|overvoltage|high voltage"
                          r"|overfrequency|prenap"),
    ("BATTERY_LOW",       r"low[- ]?float|in[- ]?discharge|battdischarge|battery discharg"
                          r"|overdischarge|over[- ]?charge|discharge|lithium battery"
                          r"|busbar ?voltage ?low|bus bar undervoltage|ubbr|napon[_ ]?sab"
                          r"|battery (current[- ]limit|temperature)|low voltage|nizak napon"
                          r"|<\s*4[0-9]|prazn|low battery|fusbat"),
    ("BATTERY_FAULT",     r"battery[- ]?(fuse|test)[- ]?(fail|break)|battery fault|fuse break"),
    ("COOLING_FAULT",     r"poorcooling|fcsoff|compressor (fault|fail)|cooling|klima|hvac"
                          r"|fan[- ]?fail|dirty filter|filter|filterblock|high ?pressure"
                          r"|low ?pressure|high ?temperature|low ?temperature|air conditioner"),
    ("DOOR_OPEN",         r"door|vrata|otvorena"),
    ("FUSE_LOAD",         r"load[- ]?fuse[- ]?fail|load_fuse_fail|mov[- ]?fail"
                          r"|system[- ]?overload|overload"),
    ("GENERIC_ERROR",     r"nurerr|urgerr|non-urgent error|presence of alarm|prisustvo alarma"
                          r"|surge voltage|svp|certificate|\berr\b"),
]
_RULES = [(cls, re.compile(pat, re.I | re.U)) for cls, pat in _RULES_RAW]

_PREFIX_RX = re.compile(r"^(BTS_|BS_|RRST_|RR_|DEA_|_DSE_)")
_MULTI_US  = re.compile(r"_+")


def _classify(text):
    for cls, rx in _RULES:
        if rx.search(text):
            return cls
    return "UNCLASSIFIED"


def _severity(s):
    t = s.strip().lower()
    if t in ("critical", "crit"):                  return "critical"
    if t in ("major", "alarm", "high"):            return "major"
    if t == "minor":                               return "minor"
    if t in ("warning", "low", "warn"):            return "warning"
    if t in ("info", "information", "node info"):  return "info"
    if t == "1": return "warning"
    if t == "2": return "minor"
    if t == "3": return "major"
    return "major"


def _transition(s):
    t = s.strip().lower()
    if t in ("cleared", "clear", "normal", "alarmnormal", "removed", "entryremoved"):
        return "clear"
    if t in ("critical", "major", "minor", "warning", "low", "active",
             "alarmactive", "added", "entryadded"):
        return "raise"
    return "instant"


def _site_key(raw):
    if not raw:
        return ""
    s = raw.strip().upper().replace(" ", "_").replace("-", "_")
    s = _MULTI_US.sub("_", s)
    s = _PREFIX_RX.sub("", s)
    return s.strip("_")


def _parse_ts(s, offset_hours=None):
    if offset_hours is None:
        offset_hours = UTC_OFFSET_HOURS
    s = re.sub(r"\s+", " ", s.strip().replace("_", " "))
    try:
        # datetime.strptime not available until Python 2.5; use time.strptime
        t = time.strptime(s, "%Y-%m-%d %H:%M:%S")
        dt = datetime.datetime(t[0], t[1], t[2], t[3], t[4], t[5])
        dt_utc = dt - datetime.timedelta(hours=offset_hours)
        return dt_utc.strftime("%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        return None


def _ts_is_future(ts_iso, tolerance_min=5):
    """Return True if ts_iso (UTC ISO string) is more than tolerance_min minutes in the future."""
    try:
        t = time.strptime(ts_iso, "%Y-%m-%dT%H:%M:%SZ")
        dt = datetime.datetime(t[0], t[1], t[2], t[3], t[4], t[5])
        return dt > datetime.datetime.utcnow() + datetime.timedelta(minutes=tolerance_min)
    except ValueError:
        return False


def _ev(source, raw_site, raw_alarm, sev_token, ts_raw, ip=None, region="", ts_offset=None):
    ts = _parse_ts(ts_raw, ts_offset)
    if not ts:
        return None
    return {
        "event_time": ts,
        "source":      source,
        "raw_site":    raw_site,
        "site_key":    _site_key(raw_site),
        "region":      region.upper(),
        "alarm_class": _classify(raw_alarm),
        "severity":    _severity(sev_token),
        "transition":  _transition(sev_token),
        "raw_alarm":   raw_alarm,
        "device_ip":   ip or None,
    }


# ── U2020 flat-line parser ─────────────────────────────────────────────────────
# Format: U2020 <SITE>  <ALARM>  <YYYY-MM-DD HH:MM:SS> <sev>
# Fields separated by 2+ spaces; timestamp in local time (CEST = UTC+2).

_U2020_RX = re.compile(
    r"^U2020\s+(\S+)\s{2,}(.+?)\s{2,}(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(\S+)\s*$"
)


def _parse_u2020(text):
    out = []
    for line in text.splitlines():
        m = _U2020_RX.match(line)
        if not m:
            continue
        raw_site, raw_alarm, ts, sev = m.groups()
        ev = _ev("u2020", raw_site, raw_alarm.strip(), sev, ts)
        if ev:
            out.append(ev)
    return out


# ── SNMP trap multi-line parser ───────────────────────────────────────────────
# Records separated by blank lines.  Header types handled:
#   ALARMRpsSc300Mib   -- Eaton RPS SC300
#   ALARMRPS-SC200-MIB -- Eaton RPS SC200
#   ALARMNetEco        -- Huawei U2020/NetEco iMAP northbound
#   ALARMBARAN klima   -- Baran AC controller
# Skipped: ALARMNetEco products-traps, ALARMDSE-74xx0, filter lines.

_SC300_RX  = re.compile(
    r"^ALARMRpsSc300Mib\s+(\S+)\s+\S+\s{2,}(\S+)\s{2,}(\S+)\s{2,}(\S+)\s+(\S+)\s+(\S+)"
)
_SC200_RX  = re.compile(
    r"^ALARMRPS-SC200-MIB\s+(\S+)\s+\S+\s{2,}(\S+)\s{2,}(\S+)\s{2,}(\S+)\s+(\S+)\s+(\S+)"
)
_NETECO_RX = re.compile(
    r"^ALARMNetEco\s+(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s{2,}(\S+)\s{2,}(.+?)\s+(\S+)\s+(\S+)\s*$"
)
_BARAN_RX  = re.compile(
    r"^ALARMBARAN klima\s+(\S+)\s+(\S+)\s{2,}(\S+)\s{2,}(\S+)\s*$"
)
_HDR_RX    = re.compile(r"^ALARM|^filter")


def _parse_oids(lines):
    d = {}
    for ln in lines:
        parts = ln.strip().split(None, 1)
        if len(parts) == 2:
            d[parts[0]] = parts[1].strip().strip('"')
    return d


def _parse_record(hdr, oid_lines):
    o = _parse_oids(oid_lines)

    m = _SC300_RX.match(hdr)
    if m:
        ts, site, region, alarm_raw, sev, ip = m.groups()
        alarm = o.get("RpsSc300Mib::trapAlarmName.0",         alarm_raw.replace("_", " "))
        sev   = o.get("RpsSc300Mib::trapAlarmKeepSeverity.0", sev)
        return _ev("rps_sc300", site, alarm, sev, ts, ip, region)

    m = _SC200_RX.match(hdr)
    if m:
        ts, site, region, alarm_raw, sev, ip = m.groups()
        return _ev("rps_sc200", site, alarm_raw.replace("_", " "), sev, ts, ip, region)

    m = _NETECO_RX.match(hdr)
    if m:
        ts_h, site_h, alarm_h, sev_h, ip = m.groups()
        site    = o.get("M2000-V1::iMAPNorthboundAlarmMOName.0",        site_h)
        alarm   = o.get("M2000-V1::iMAPNorthboundAlarmProbablecause.0", alarm_h.strip())
        sev     = o.get("M2000-V1::iMAPNorthboundAlarmLevel.0",         sev_h)
        restore = o.get("M2000-V1::iMAPNorthboundAlarmRestore.0", "")
        if restore.lower() == "cleared":
            sev = "cleared"
            ts_oid = (o.get("M2000-V1::iMAPNorthboundAlarmRestoreTime.0") or
                      o.get("M2000-V1::iMAPNorthboundAlarmOccurTime.0"))
        else:
            ts_oid = o.get("M2000-V1::iMAPNorthboundAlarmOccurTime.0")
        # NetEco OID timestamps (OccurTime/RestoreTime) are in UTC → offset 0.
        # Header ts_h is the SNMP manager's local time (CEST) → UTC_OFFSET_HOURS.
        # If the OID value is > 5 min in the future (NetEco sends scheduled repair
        # times, not actual times), fall back to the header clock which is accurate.
        if ts_oid:
            candidate = _parse_ts(ts_oid, 0)
            if candidate and not _ts_is_future(candidate):
                ts, ts_off = ts_oid, 0    # OID UTC is plausible
            else:
                ts, ts_off = ts_h, None   # OID is future → use CEST header
        else:
            ts, ts_off = ts_h, None
        if not site or not alarm:
            return None
        return _ev("net_eco", site, alarm, sev, ts, ip, ts_offset=ts_off)

    m = _BARAN_RX.match(hdr)
    if m:
        ts, alarm_raw, status, ip = m.groups()
        if status.upper() == "ACTIVE":
            sev = "major"
        else:
            sev = "cleared"
        return _ev("baran", ip, alarm_raw.replace("_", " "), sev, ts, ip)

    return None   # filter / DSE-74xx / products-traps -> skip


def _parse_snmp(text):
    out, hdr, oid_lines = [], None, []
    for line in text.splitlines():
        if line == "":
            if hdr:
                ev = _parse_record(hdr, oid_lines)
                if ev:
                    out.append(ev)
                hdr, oid_lines = None, []
        elif _HDR_RX.match(line):
            if hdr:   # flush previous (header immediately follows header)
                ev = _parse_record(hdr, oid_lines)
                if ev:
                    out.append(ev)
            hdr, oid_lines = line, []
        elif hdr is not None:
            oid_lines.append(line)
    if hdr:   # flush last record (no trailing blank line at EOF)
        ev = _parse_record(hdr, oid_lines)
        if ev:
            out.append(ev)
    return out


# ── File reading ───────────────────────────────────────────────────────────────

def _read_chunk(path, fmt, offset):
    """Read new bytes from offset, trimmed to last complete record boundary.
    Returns (decoded_text, new_offset).  Empty text means nothing new.
    """
    try:
        size = os.path.getsize(path)
    except OSError, e:
        _log("stat %s: %s" % (os.path.basename(path), e))
        return "", offset

    if size < offset:
        _log("%s shrank -- resetting offset to 0" % os.path.basename(path))
        offset = 0

    if size == offset:
        return "", offset

    f = open(path, "rb")
    try:
        f.seek(offset)
        raw = f.read(size - offset)
    finally:
        f.close()

    # Trim to last complete record so we never split mid-line or mid-record
    if fmt == "u2020_flat":
        cut = raw.rfind("\n")
        if cut == -1:
            return "", offset
        end = cut + 1
    else:   # snmp_trap: records separated by blank line (\n\n)
        cut = raw.rfind("\n\n")
        if cut == -1:
            return "", offset
        end = cut + 2   # include both \n so last record ends with blank line

    return raw[:end].decode("utf-8", "replace"), offset + end


# ── HTTP ───────────────────────────────────────────────────────────────────────

def _post(events):
    data = _jdumps(events)
    req  = urllib2.Request(API_URL, data, {"Content-Type": "application/json"})
    try:
        r = urllib2.urlopen(req)
        try:
            res = _jloads(r.read())
        finally:
            r.close()
        _log("posted %d events -- inserted=%s" % (len(events), res.get("inserted", "?")))
        return True
    except urllib2.URLError, e:
        _log("POST failed: %s" % e)
        return False


# ── Poll cycle ─────────────────────────────────────────────────────────────────

def poll(catchup=False):
    state       = _load_state()
    new_offsets = {}    # path -> candidate new offset
    path_events = {}    # path -> list[event]  (only paths that had new content)

    for path, fmt in FILES:
        if path not in state and not catchup:
            # First run: park at current end of file to avoid ingesting history.
            try:
                sz = os.path.getsize(path)
            except OSError:
                sz = 0
            new_offsets[path] = sz
            _log("%s: first run -- parked at byte %d (re-run with --catchup to ingest history)" % (
                os.path.basename(path), sz))
            continue

        text, new_off = _read_chunk(path, fmt, state.get(path, 0))
        new_offsets[path] = new_off

        if not text:
            continue

        if fmt == "u2020_flat":
            parsed = _parse_u2020(text)
        else:
            parsed = _parse_snmp(text)
        path_events[path] = parsed
        if parsed:
            _log("%s: %d new events (offset %d -> %d)" % (
                os.path.basename(path), len(parsed), state.get(path, 0), new_off))

    all_events = []
    for evs in path_events.values():
        all_events.extend(evs)

    merged = dict(state)
    merged.update(new_offsets)

    if not all_events:
        _save_state(merged)
        return

    # POST in batches; on failure roll back offsets for files that had events
    success = True
    for i in range(0, len(all_events), MAX_BATCH):
        if not _post(all_events[i : i + MAX_BATCH]):
            success = False
            break

    if not success:
        for path in path_events:
            merged[path] = state.get(path, 0)   # roll back
    _save_state(merged)


# ── Entry point ────────────────────────────────────────────────────────────────

def main():
    once    = "--once"    in sys.argv
    catchup = "--catchup" in sys.argv
    _log("snmp_log_poller starting  api=%s  interval=%ds  catchup=%s" % (
        API_URL, POLL_SECS, catchup))
    if once:
        poll(catchup)
        return
    while True:
        try:
            poll(catchup)
        except Exception, e:
            _log("unhandled error: %s" % e)
        time.sleep(POLL_SECS)


if __name__ == "__main__":
    main()
