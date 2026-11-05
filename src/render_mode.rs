//! 启动时选择 glow（OpenGL）或 wgpu。虚拟显示器上常见的微软 OpenGL 1.1
//! 不能跑 egui_glow，这时改用 wgpu，并在 Windows 上优先 DirectX 12。

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Glow,
    Wgpu,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuClass {
    Discrete,
    Integrated,
    Virtual,
    Other,
    Cpu,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveRender {
    pub name: &'static str,
    pub detail: String,
    /// `Some(true)` 表示交换链提供了预乘或直乘 alpha。
    pub transparent: Option<bool>,
}

static ACTIVE: OnceLock<ActiveRender> = OnceLock::new();
static TRANSPARENCY: OnceLock<Option<bool>> = OnceLock::new();

/// 解析 `DAOZAI_RENDERER`。空值视为 `auto`。无法识别时仍回落到 `auto`，并带回说明。
pub fn preference_from(value: Option<&str>) -> (Preference, Option<String>) {
    let Some(raw) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return (Preference::Auto, None);
    };
    match raw.to_ascii_lowercase().as_str() {
        "auto" => (Preference::Auto, None),
        "glow" => (Preference::Glow, None),
        "wgpu" => (Preference::Wgpu, None),
        _ => (
            Preference::Auto,
            Some(format!("DAOZAI_RENDERER={raw} 无法识别，将按 auto 启动。可用值为 glow、wgpu、auto")),
        ),
    }
}

pub fn preference_label(preference: Preference) -> &'static str {
    match preference {
        Preference::Glow => "glow",
        Preference::Wgpu => "wgpu",
        Preference::Auto => "auto",
    }
}

/// 硬件适配器优先，仅有软件光栅（WARP）时才用 CPU。
pub fn gpu_preference(class: GpuClass) -> u8 {
    match class {
        GpuClass::Discrete => 0,
        GpuClass::Integrated => 1,
        GpuClass::Virtual => 2,
        GpuClass::Other => 3,
        GpuClass::Cpu => 4,
    }
}

pub fn note_transparency(value: Option<bool>) {
    let _ = TRANSPARENCY.set(value);
}

pub fn noted_transparency() -> Option<bool> {
    TRANSPARENCY.get().copied().flatten()
}

pub fn set_active(render: ActiveRender) {
    let _ = ACTIVE.set(render);
}

pub fn active() -> Option<&'static ActiveRender> {
    ACTIVE.get()
}

pub fn settings_hint() -> Option<String> {
    hint(ACTIVE.get())
}

pub fn hint(active: Option<&ActiveRender>) -> Option<String> {
    let active = active.filter(|render| render.name == "wgpu")?;
    Some(match active.transparent {
        Some(true) => "当前为兼容渲染模式（DirectX）。窗口仍是无边框、置顶，空白处点击会穿透；这块显卡报告支持透明合成。".into(),
        Some(false) => "当前为兼容渲染模式（DirectX）。此设备不能把窗口背景做成透明，圆角外可能出现底色；无边框、置顶和点击穿透仍然有效。有 OpenGL 的电脑可设置环境变量 DAOZAI_RENDERER=glow。".into(),
        None => "当前为兼容渲染模式（DirectX）。若圆角外出现底色，说明此设备不能透明合成；无边框、置顶和点击穿透仍然有效。".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_auto_select_glow_first() {
        assert_eq!(preference_from(None).0, Preference::Auto);
        assert_eq!(preference_from(Some("  ")).0, Preference::Auto);
        assert_eq!(preference_from(Some("AuTo")).0, Preference::Auto);
        assert!(preference_from(Some("auto")).1.is_none());
    }

    #[test]
    fn explicit_renderer_names_are_case_insensitive() {
        assert_eq!(preference_from(Some(" glow ")).0, Preference::Glow);
        assert_eq!(preference_from(Some("WGPU")).0, Preference::Wgpu);
    }

    #[test]
    fn unknown_renderer_falls_back_to_auto_with_a_note() {
        let (preference, note) = preference_from(Some("dx12"));
        assert_eq!(preference, Preference::Auto);
        let note = note.unwrap();
        assert!(note.contains("dx12"));
        assert!(note.contains("glow、wgpu、auto"));
    }

    #[test]
    fn software_adapters_rank_after_hardware() {
        assert!(gpu_preference(GpuClass::Discrete) < gpu_preference(GpuClass::Integrated));
        assert!(gpu_preference(GpuClass::Integrated) < gpu_preference(GpuClass::Virtual));
        assert!(gpu_preference(GpuClass::Virtual) < gpu_preference(GpuClass::Cpu));
    }

    #[test]
    fn settings_hint_only_appears_for_the_compatibility_renderer() {
        assert!(hint(None).is_none());
        assert!(hint(Some(&ActiveRender { name: "glow", detail: String::new(), transparent: Some(true) })).is_none());
        let opaque = hint(Some(&ActiveRender { name: "wgpu", detail: String::new(), transparent: Some(false) })).unwrap();
        assert!(opaque.contains("兼容渲染模式"));
        assert!(opaque.contains("DAOZAI_RENDERER=glow"));
        let clear = hint(Some(&ActiveRender { name: "wgpu", detail: String::new(), transparent: Some(true) })).unwrap();
        assert!(clear.contains("穿透"));
        assert!(clear.contains("透明合成"));
        assert!(!clear.contains("底色"));
    }
}
