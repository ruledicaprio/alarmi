-- Dimension seed data (idempotent).
INSERT INTO dim_source (source, label, is_stateful, description) VALUES
 ('ignition','IgnitionSCADA',     FALSE,'k8s SCADA severity feed (count-only)'),
 ('net_eco','Huawei NetEco',      FALSE,'NetEco alarms; raises only in feed (count-only)'),
 ('u2020','Huawei U2020',         TRUE ,'U2020 dry-contact alarms; major/cleared'),
 ('rps_sc200','Eaton SC200 (RPS)',TRUE ,'SC200 system controller via RPS-SC200-MIB'),
 ('rps_sc300','Eaton SC300 (RPS)',TRUE ,'SC300 system controller via RpsSc300Mib'),
 ('dse74xx','DSE 7410/7420',      TRUE ,'Genset controller SNMP'),
 ('benning','Benning rectifier',  TRUE ,'DCMCUMIB added/removed traps'),
 ('baran','BARAN FCS cooling',    TRUE ,'Cooling controller active/normal'),
 ('modbus_eaton','Modbus SC200/300',TRUE,'Direct Modbus poll (later stage)'),
 ('html_oos','Out-of-service table',TRUE,'/alarmi/ service-outage table by technology')
ON CONFLICT (source) DO UPDATE
  SET label=EXCLUDED.label, is_stateful=EXCLUDED.is_stateful, description=EXCLUDED.description;

INSERT INTO dim_alarm_class (alarm_class, label, is_power_critical, default_severity, description) VALUES
 ('MAINS_FAILURE','Mains / AC failure',     TRUE ,'critical','Loss of AC mains, phase fail/undervoltage, blackout'),
 ('RECTIFIER_FAILURE','Rectifier failure',  TRUE ,'major','Rectifier/SMPS power failure'),
 ('RECTIFIER_COMMS','Rectifier comms lost', FALSE,'minor','Rectifier communication lost'),
 ('BATTERY_LOW','Battery low / discharge',  TRUE ,'major','Discharge, low float, <48V, overdischarge'),
 ('BATTERY_FAULT','Battery fault',          TRUE ,'major','Battery fuse/test fail'),
 ('SOLAR_FAULT','Solar fault',              FALSE,'minor','Solar/PV fault or comms lost'),
 ('GENSET_EVENT','Genset event',            TRUE ,'major','Engine start/stop, genset alarms, fuel'),
 ('UPS_MODULE','UPS / inverter module',     TRUE ,'major','UPS module / inverter fault'),
 ('COOLING_FAULT','Cooling fault',          TRUE ,'major','HVAC/FCS poor cooling, compressor, fan'),
 ('HIGH_VOLTAGE','Overvoltage',             TRUE ,'major','Overvoltage / overfrequency'),
 ('DOOR_OPEN','Door open',                  FALSE,'warning','Cabinet/site door open'),
 ('FUSE_LOAD','Load fuse / overload',       TRUE ,'major','Load fuse fail, overload, MOV fail'),
 ('NE_DISCONNECTED','NE disconnected',      FALSE,'major','Network element unreachable'),
 ('COMMS_LOST','Comms lost',                FALSE,'minor','Communication lost (dominant Ignition noise class)'),
 ('SERVICE_OUTAGE','Service outage',        TRUE ,'critical','Out-of-service per technology (PRISTUP/BTS/...)'),
 ('GENERIC_ERROR','Generic error',          FALSE,'warning','Non-urgent / presence-of-alarm / misc'),
 ('UNCLASSIFIED','Unclassified',            FALSE,'warning','Did not match taxonomy; review candidate')
ON CONFLICT (alarm_class) DO UPDATE
  SET label=EXCLUDED.label, is_power_critical=EXCLUDED.is_power_critical,
      default_severity=EXCLUDED.default_severity, description=EXCLUDED.description;
