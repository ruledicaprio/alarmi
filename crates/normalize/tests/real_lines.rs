//! Tests use verbatim lines from the real master_alarms.log so the Rust
//! parser stays in lock-step with the validated Python oracle.

use bht_normalize::{normalize_line, parse_oos_table, AlarmClass, Severity, Source, Transition};

#[test]
fn ignition_critical_is_raise() {
    let l = r#"IgnitionSCADA,   Sarajevo - DMalta ,   "8000041",   UPS 2 S1 ALARMI modula 37 ,   Critical , 2026-04-14_01:53:38"#;
    let e = normalize_line(l).unwrap();
    assert_eq!(e.source, Source::Ignition);
    assert_eq!(e.region, "SARAJEVO");
    assert_eq!(e.site_key, "DMALTA");
    assert_eq!(e.alarm_class, AlarmClass::UpsModule);
    assert_eq!(e.severity, Severity::Critical);
    assert_eq!(e.transition, Transition::Raise); // status field critical -> raise
}

#[test]
fn ignition_gubitak_komunikacije_is_comms_lost() {
    // The single most common alarm in the feed (~135k rows), Bosnian wording.
    let l = "IgnitionSCADA,   Tuzla - HOST Pasa Bunar ,   \"2000028\",   Gubitak komunikacije ,   Critical , 2026-04-14_03:03:53";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.alarm_class, AlarmClass::CommsLost);
}

#[test]
fn u2020_rectifier_pairs_raise_and_clear() {
    let raise = "U2020 , PODBARE ,  Kvar ispravljaca (Rectifier Failure) , 2026-04-14 02:10:44 , major , ";
    let clear = "U2020 , PODBARE ,  Kvar ispravljaca (Rectifier Failure) , 2026-04-14 02:09:40 , cleared , ";
    let r = normalize_line(raise).unwrap();
    let c = normalize_line(clear).unwrap();
    assert_eq!(r.alarm_class, AlarmClass::RectifierFailure);
    assert_eq!(r.transition, Transition::Raise);
    assert_eq!(c.transition, Transition::Clear);
    assert_eq!(r.site_key, "PODBARE");
}

#[test]
fn u2020_nestanak_220_is_mains_failure() {
    let l = "U2020 , VUCJA_LUKA ,  Nestanak 220 V  , 2026-04-14 01:39:56 , major , ";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.alarm_class, AlarmClass::MainsFailure);
    assert_eq!(e.site_key, "VUCJA_LUKA");
}

#[test]
fn neteco_legacy_4field_is_raise() {
    let l = "NetEco , KISELJAK_CENTAR ,  Mains Phase L3 Failure , 2026-04-14 01:44:59";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.source, Source::NetEco);
    assert_eq!(e.alarm_class, AlarmClass::MainsFailure);
    assert_eq!(e.transition, Transition::Raise);
}

#[test]
fn rps_sc300_has_region_ip_and_pairs() {
    let l = "RpsSc300Mib ,  BOS_GRAHOVO ,  BOSANSKO_GRAHOVO ,  AC_Fail , systemAlarm , 2026-04-13_19:56:42 , 10.10.6.254, cleared";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.source, Source::RpsSc300);
    assert_eq!(e.region, "BOSANSKO_GRAHOVO");
    assert_eq!(e.alarm_class, AlarmClass::MainsFailure);
    assert_eq!(e.transition, Transition::Clear);
    assert_eq!(e.device_ip.as_deref(), Some("10.10.6.254"));
}

#[test]
fn benning_timestamp_is_last_field() {
    let l = "Benning_napajanje , PASA_BUNAR_I , DCMCUMIB::dcTrapAlarmEntryRemoved , DCMCUMIB::dcAlarmACInputFault 3 , 3 , 10.10.2.61, 2026-04-13_08:41:22";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.source, Source::Benning);
    assert_eq!(e.transition, Transition::Clear); // "Removed"
    assert_eq!(e.alarm_class, AlarmClass::MainsFailure); // AC Input Fault
    assert_eq!(e.device_ip.as_deref(), Some("10.10.2.61"));
    // 08:41:22 CEST -> 06:41:22 UTC
    assert_eq!(e.event_time.to_rfc3339(), "2026-04-13T06:41:22+00:00");
}

#[test]
fn baran_cooling_fault() {
    let l = "BARAN_klima , RR_Tusnica_Livno , ACTIVE ,  , poorcooling , 2026-04-14_01:05:32 , 10.10.6.83, major";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.alarm_class, AlarmClass::CoolingFault);
    assert_eq!(e.transition, Transition::Raise);
    assert_eq!(e.site_key, "TUSNICA_LIVNO"); // RR_ prefix stripped
}

#[test]
fn dse_genset_event() {
    let l = "DSE-74xx, DEA_Visibaba , _DSE_RR_BS_Visibaba ,  , singleEventNotification , notifEngineStarts , 2026-04-13_13:54:05 , 10.10.4.71, major";
    let e = normalize_line(l).unwrap();
    assert_eq!(e.source, Source::Dse74xx);
    assert_eq!(e.alarm_class, AlarmClass::GensetEvent);
    assert_eq!(e.site_key, "VISIBABA"); // DEA_ prefix stripped
}

#[test]
fn junk_lines_are_dropped() {
    assert!(normalize_line("").is_err());
    assert!(normalize_line("no comma here").is_err());
    assert!(normalize_line("nepoznat_alarm,  ,  ,  ,  , 2026-04-13_18:50:49 , 10.10.3.133,  SNMPv2-MIB").is_err());
}

#[test]
fn oos_table_sections_and_rows() {
    let lines = vec![
        "-------------------------BTS------------------------------",
        "BTS_POTOCARI_MUZEJ 2026-06-16 12:59:00 Tuzla",
        "BTS_DRZIREP 2026-06-11 21:22:00 Mostar",
        "-------------------------MPLS-----------------------------",
        "VK VLADA FBIH-BS JUMBO POFALICI 2026-06-16 15:58:00 SARAJEVO",
    ];
    let evs = parse_oos_table(&lines);
    assert_eq!(evs.len(), 3);
    assert_eq!(evs[0].alarm_class, AlarmClass::ServiceOutage);
    assert_eq!(evs[0].raw_alarm, "OUT_OF_SERVICE:BTS");
    assert_eq!(evs[0].site_key, "POTOCARI_MUZEJ");
    assert_eq!(evs[0].region, "TUZLA");
    assert_eq!(evs[2].raw_alarm, "OUT_OF_SERVICE:MPLS");
}

#[test]
fn ignition_cleared_is_clear() {
    let l = r#"IgnitionSCADA,   Bihac - BS Cadjavica ,   "4000035",   Gubitak komunikacije ,   cleared , 2026-06-17_05:52:27"#;
    let e = normalize_line(l).unwrap();
    assert_eq!(e.alarm_class, AlarmClass::CommsLost);
    assert_eq!(e.transition, Transition::Clear);
    assert_eq!(e.site_key, "CADJAVICA");
}

#[test]
fn neteco_live_5field_pairs() {
    let raise = "NetEco , CELINAC_BOJICI ,  AC Failure , 2026-06-17 05:41:12 , critical";
    let clear = "NetEco , CELINAC_BOJICI ,  AC Failure , 2026-06-17 05:44:31 , cleared";
    let r = normalize_line(raise).unwrap();
    let c = normalize_line(clear).unwrap();
    assert_eq!(r.alarm_class, AlarmClass::MainsFailure);
    assert_eq!(r.transition, Transition::Raise);
    assert_eq!(c.transition, Transition::Clear);
}

#[test]
fn dse_mains_fail_and_return_pair() {
    let fail = "DSE-74xx, DEA_Alaginci , _DSE_RR_Alaginci ,  , singleEventNotification , notifMainsFail , 2026-06-17_05:44:04 , 10.10.4.69, major";
    let ret  = "DSE-74xx, DEA_Alaginci , _DSE_RR_Alaginci ,  , singleEventNotification , notifMainsReturn , 2026-06-17_05:51:54 , 10.10.4.69, clear";
    let f = normalize_line(fail).unwrap();
    let r = normalize_line(ret).unwrap();
    assert_eq!(f.alarm_class, AlarmClass::MainsFailure);
    assert_eq!(f.transition, Transition::Raise);
    assert_eq!(r.alarm_class, AlarmClass::MainsFailure); // notifmains -> pairs with fail
    assert_eq!(r.transition, Transition::Clear);
    assert_eq!(f.site_key, "ALAGINCI");
}
