//! Tiny Chinese → English translation map for Huawei SmartLogger / Sun2000
//! status and alarm text. Extend as new terms show up in real polls.

use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static ZH_TO_EN: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // device states
    m.insert("运行",               "running");
    m.insert("正常运行",           "normal_running");
    m.insert("待机",               "standby");
    m.insert("启动中",             "starting");
    m.insert("启动",               "starting");
    m.insert("停机",               "shutdown");
    m.insert("停机命令",           "shutdown_command");
    m.insert("故障",               "fault");
    m.insert("告警",               "alarm");
    m.insert("孤岛",               "islanding");
    m.insert("并网",               "on_grid");
    m.insert("离网",               "off_grid");
    m.insert("电网检测",           "grid_detecting");
    m.insert("绝缘阻抗检测",       "insulation_check");

    // grid / power faults
    m.insert("电网欠压",           "grid_undervoltage");
    m.insert("电网过压",           "grid_overvoltage");
    m.insert("电网欠频",           "grid_underfrequency");
    m.insert("电网过频",           "grid_overfrequency");
    m.insert("电网失压",           "grid_voltage_loss");
    m.insert("电网失频",           "grid_frequency_loss");
    m.insert("电网异常",           "grid_abnormal");
    m.insert("功率限制",           "power_limited");
    m.insert("功率降额",           "power_derating");

    // string / DC side
    m.insert("组串输入电压高",     "string_input_voltage_high");
    m.insert("组串反接",           "string_reverse");
    m.insert("组串电流反灌",       "string_current_backfeed");
    m.insert("直流过流",           "dc_overcurrent");
    m.insert("绝缘阻抗低",         "insulation_low");
    m.insert("接地故障",           "ground_fault");
    m.insert("漏电",               "leakage_current");
    m.insert("漏电流过大",         "leakage_current_high");

    // ambient / device
    m.insert("过温",               "over_temperature");
    m.insert("温度过高",           "temperature_high");
    m.insert("风扇故障",           "fan_fault");
    m.insert("通信中断",           "communication_lost");
    m.insert("通信失败",           "communication_failed");

    // misc
    m.insert("未知",               "unknown");
    m.insert("无",                 "none");
    m.insert("是",                 "yes");
    m.insert("否",                 "no");

    m
});

/// Best-effort translate: replace every Chinese phrase we know with its English
/// equivalent. Untranslated bytes are left as-is.
pub fn translate(text: &str) -> String {
    let mut s = text.to_string();
    for (zh, en) in ZH_TO_EN.iter() {
        if s.contains(zh) {
            s = s.replace(zh, en);
        }
    }
    s
}
