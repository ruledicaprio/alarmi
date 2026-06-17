#!/usr/bin/env python3
"""
Stage-1 normalization REFERENCE implementation (Python).
Purpose: prove the canonical model against the real master_alarms.log before
the Rust crate is written to mirror this exact logic.

  CanonicalEvent = (event_time_utc, source, raw_site, site_key, region,
                    alarm_class, severity, transition, raw_alarm, device_ip)

This file is a SPEC ORACLE, not production code. crates/normalize mirrors
these rules 1:1 and is the production parser.
"""
import re, sys
from collections import Counter
from datetime import datetime, timezone, timedelta

LOCAL_OFFSET = timezone(timedelta(hours=2))  # Sarajevo CEST; DST refinement = later seam
RAISE, CLEAR, INSTANT = "RAISE", "CLEAR", "INSTANT"

# Alarm-class taxonomy: ordered (first match wins). EN + Bosnian wording.
CLASS_RULES = [
    ("NE_DISCONNECTED",    r"\bne is disconnected\b|\bdisconnected\b"),
    ("COMMS_LOST",         r"gubitak komunikacije|prisustvo komunikacije|comms[- ]?lost|communication|snmpv2-mib|nepoznat|node info"),
    ("GENSET_EVENT",       r"engine ?(start|stop)|generator ?(start|stop|enable|over|under)|notifengine|notiflevel|levelstatus|namedalarm|\bdse\b|\bdea[_ ]|nivo goriva|fuel|oilpressure"),
    ("MAINS_FAILURE",      r"nestanak 220|mains ?(failure|fail)|mains phase l[123]|mains\d*voltage|ac[_ ]?fail|acinputfault|ac input fault|input fault|ac phase l[123]|partial[- ]ac[- ]fail|phase[- ]?fail|phase l[123]|under ?voltage|nestanak (mreže|mreze)|undervoltage|ispad faze|ispad[_ ]?mreze|blackout"),
    ("RECTIFIER_FAILURE",  r"kvar ispravlja|rectifier (power )?(failure|fail)|rectifier[- ]fail|ispravlja"),
    ("RECTIFIER_COMMS",    r"rectifier[- ]comms[- ]lost"),
    ("SOLAR_FAULT",        r"solar[_ ]?fail|solar[- ]comms[- ]lost"),
    ("UPS_MODULE",         r"ups .*modula|ups .*module|ups[- ]?fail|alarmi modula|inverter (fault|fail)|ispad[_ ]?invertora|invertor|bypass"),
    ("HIGH_VOLTAGE",       r"visok napon|over[- ]?voltage|overvoltage|high voltage|overfrequency|prenap"),
    ("BATTERY_LOW",        r"low[- ]?float|in[- ]?discharge|battdischarge|battery discharg|overdischarge|over[- ]?charge|discharge|lithium battery|busbar ?voltage ?low|bus bar undervoltage|ubbr|napon[_ ]?sab|battery (current[- ]limit|temperature)|low voltage|nizak napon|<\s*4[0-9]|prazn|low battery|fusbat"),
    ("BATTERY_FAULT",      r"battery[- ]?(fuse|test)[- ]?(fail|break)|battery fault|fuse break"),
    ("COOLING_FAULT",      r"poorcooling|fcsoff|compressor (fault|fail)|cooling|klima|hvac|fan[- ]?fail|dirty filter|filter|filterblock|high ?pressure|low ?pressure|high ?temperature|low ?temperature|air conditioner"),
    ("DOOR_OPEN",          r"door|vrata|otvorena"),
    ("FUSE_LOAD",          r"load[- ]?fuse[- ]?fail|load_fuse_fail|mov[- ]?fail|system[- ]?overload|overload"),
    ("GENERIC_ERROR",      r"nurerr|urgerr|non-urgent error|presence of alarm|prisustvo alarma|surge voltage|svp|certificate|\berr\b"),
    ("SERVICE_OUTAGE",    r"^__oos__"),
]
CLASS_RX = [(c, re.compile(p, re.I)) for c, p in CLASS_RULES]

def classify(raw_text, hint=None):
    if hint == "oos":
        return "SERVICE_OUTAGE"
    for cls, rx in CLASS_RX:
        if rx.search(raw_text or ""):
            return cls
    return "UNCLASSIFIED"

def norm_sev(s):
    s = (s or "").strip().lower()
    if s in ("critical","crit"): return "critical"
    if s in ("major","alarm","high"): return "major"
    if s in ("minor",): return "minor"
    if s in ("warning","low","warn"): return "warning"
    if s in ("info","information","node info"): return "info"
    if s.isdigit(): return {"1":"warning","2":"minor","3":"major"}.get(s,"minor")
    return "major"

STATEFUL_SOURCES = {"u2020","rps_sc200","rps_sc300","benning","baran","dse74xx","modbus_eaton"}
def norm_transition(source, status):
    if source not in STATEFUL_SOURCES:
        return INSTANT
    s = (status or "").strip().lower()
    if s in ("cleared","clear","normal","alarmnormal","removed","entryremoved"): return CLEAR
    if s in ("major","critical","active","alarmactive","added","entryadded"): return RAISE
    return INSTANT

def parse_ts(s):
    if not s: return None
    s = re.sub(r"\s+", " ", s.strip().replace("_", " "))
    for fmt in ("%Y-%m-%d %H:%M:%S", "%d %b %Y %H:%M:%S", "%Y-%m-%d %H:%M"):
        try: return datetime.strptime(s, fmt).replace(tzinfo=LOCAL_OFFSET).astimezone(timezone.utc)
        except ValueError: continue
    return None

def site_key(raw):
    if not raw: return ""
    s = raw.strip().upper()
    s = re.sub(r"^(BTS_|BS_|RRST_|RR_|DEA_|_DSE_)", "", s)
    s = s.replace(" ", "_").replace("-", "_")
    return re.sub(r"_+", "_", s).strip("_")

def split_csv(line):
    return [p.strip().strip('"').strip() for p in line.split(",")]

def p_ignition(f):
    if len(f) < 6: return None
    region, site = "", f[1]
    m = re.match(r"^([A-Za-zŠšĐđČčĆćŽž]+)\s*-\s*(.+)$", f[1])
    if m: region, site = m.group(1).strip(), m.group(2).strip()
    return dict(source="ignition", raw_site=f[1], site=site_key(site), region=region.upper(),
               raw_alarm=f[3], code=f[2], sev=f[4], status=f[4], ts=f[5], ip="")

def p_neteco(f):
    if len(f) < 4: return None
    return dict(source="neteco", raw_site=f[1], site=site_key(f[1]), region="",
               raw_alarm=f[2], code="", sev="major", status="active", ts=f[3], ip="")

def p_u2020(f):
    if len(f) < 5: return None
    return dict(source="u2020", raw_site=f[1], site=site_key(f[1]), region="",
               raw_alarm=f[2], code="", sev=f[4], status=f[4], ts=f[3], ip="")

def p_rps(f, src):
    if len(f) < 8: return None
    return dict(source=src, raw_site=f[1], site=site_key(f[1]), region=f[2].upper(),
               raw_alarm=f[3], code="", sev=f[7], status=f[7], ts=f[5], ip=f[6])

def p_dse(f):
    if len(f) < 9: return None
    return dict(source="dse74xx", raw_site=f[1], site=site_key(f[1]), region="",
               raw_alarm=(f[5] or f[4]), code="", sev=f[8], status=f[4], ts=f[6], ip=f[7])

def p_benning(f):
    if len(f) < 7: return None
    status = "removed" if "Removed" in f[2] else "added"
    return dict(source="benning", raw_site=f[1], site=site_key(f[1]), region="",
               raw_alarm=f[3], code="", sev=f[4], status=status, ts=f[6], ip=f[5])

def p_baran(f):
    if len(f) < 8: return None
    return dict(source="baran", raw_site=f[1], site=site_key(f[1]), region="",
               raw_alarm=f[4], code="", sev=f[7], status=f[7], ts=f[5], ip=f[6])

DISPATCH = {
    "IgnitionSCADA": p_ignition, "NetEco": p_neteco, "U2020": p_u2020,
    "RPS-SC200-MIB": lambda f: p_rps(f,"rps_sc200"),
    "RpsSc300Mib":   lambda f: p_rps(f,"rps_sc300"),
    "DSE-74xx": p_dse, "Benning_napajanje": p_benning, "BARAN_klima": p_baran,
}

def normalize_line(line):
    line = line.lstrip("﻿").rstrip()
    if not line or "," not in line:
        return None, "blank_or_nocomma"
    f = split_csv(line)
    fn = DISPATCH.get(f[0])
    if fn is None: return None, "unknown_system:" + f[0][:24]
    rec = fn(f)
    if rec is None: return None, "field_count:" + f[0]
    ts = parse_ts(rec["ts"])
    if ts is None: return None, "bad_ts:" + f[0]
    return dict(
        event_time=ts.isoformat(), source=rec["source"], raw_site=rec["raw_site"],
        site_key=rec["site"], region=rec.get("region",""),
        alarm_class=classify(rec["raw_alarm"]), severity=norm_sev(rec["sev"]),
        transition=norm_transition(rec["source"], rec["status"]), raw_alarm=rec["raw_alarm"],
        device_ip=rec.get("ip",""),
    ), "ok"

def main(path):
    total=ok=0
    reasons=Counter(); by_source=Counter(); by_class=Counter()
    by_sev=Counter(); by_trans=Counter(); sites=set(); unclassified=Counter()
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            total+=1
            ev, why = normalize_line(line)
            if ev is None:
                reasons[why.split(":")[0]]+=1; continue
            ok+=1
            by_source[ev["source"]]+=1; by_class[ev["alarm_class"]]+=1
            by_sev[ev["severity"]]+=1;  by_trans[ev["transition"]]+=1
            sites.add(ev["site_key"])
            if ev["alarm_class"]=="UNCLASSIFIED":
                unclassified[ev["raw_alarm"][:48]]+=1
    print("TOTAL lines     : %d" % total)
    print("NORMALIZED ok   : %d  (%.2f%%)" % (ok, 100*ok/total))
    print("DROPPED         : %d  reasons=%s" % (total-ok, dict(reasons)))
    print("DISTINCT sites  : %d" % len(sites))
    print("\nBY SOURCE       : %s" % dict(by_source.most_common()))
    print("\nBY ALARM_CLASS  :")
    for c,n in by_class.most_common(): print("    %-20s %d" % (c,n))
    classified = ok - by_class.get('UNCLASSIFIED',0)
    print("\nCLASSIFIED      : %d/%d (%.2f%%)" % (classified, ok, 100*classified/ok))
    print("BY SEVERITY     : %s" % dict(by_sev.most_common()))
    print("BY TRANSITION   : %s" % dict(by_trans.most_common()))
    print("\nTOP UNCLASSIFIED raw alarms:")
    for t,n in unclassified.most_common(15): print("    %6d  %s" % (n,t))

if __name__=="__main__":
    main(sys.argv[1] if len(sys.argv)>1 else "master_alarms.log")
