//! 图形启动：默认先试 OpenGL，上下文或版本不够时再试 wgpu。

use std::cell::Cell;
use std::rc::Rc;

use eframe::egui;

use daozai::render_mode::{self, Preference};

pub fn run_app(create: fn(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>) -> Result<(), String> {
    let (preference, note) = render_mode::preference_from(std::env::var("DAOZAI_RENDERER").ok().as_deref());
    if let Some(note) = note {
        crate::diagnostics::note(&note);
    }
    crate::diagnostics::startup(&format!("初始化图形窗口（{}）", render_mode::preference_label(preference)));
    match preference {
        Preference::Glow => run_once(eframe::Renderer::Glow, create).map_err(|error| error.to_string()),
        Preference::Wgpu => run_once(eframe::Renderer::Wgpu, create).map_err(|error| error.to_string()),
        Preference::Auto => run_auto(create),
    }
}

fn run_auto(create: fn(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>) -> Result<(), String> {
    let (started, glow) = run_once_tracking(eframe::Renderer::Glow, create);
    if started {
        return glow.map_err(|error| error.to_string());
    }
    let Err(glow_error) = glow else { return Ok(()) };
    if !graphics_context_failed(&glow_error) {
        return Err(glow_error.to_string());
    }
    crate::diagnostics::record_failure(
        "OpenGL 不可用，正在尝试兼容渲染（DirectX / wgpu）。",
        &format!("若程序停在这里没有出现岛仔，请把本文件发给开发者。\n可设置 DAOZAI_RENDERER=glow、wgpu 或 auto（默认 auto）。\n\n{glow_error}"),
    );
    crate::diagnostics::startup("OpenGL 不可用，改用兼容渲染");
    match run_once(eframe::Renderer::Wgpu, create) {
        Ok(()) => Ok(()),
        Err(wgpu_error) => Err(format!(
            "OpenGL 启动失败：{glow_error}\n\n已尝试兼容渲染（DirectX / wgpu），仍然失败：{wgpu_error}"
        )),
    }
}

fn run_once(
    renderer: eframe::Renderer,
    create: fn(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> eframe::Result {
    run_once_tracking(renderer, create).1
}

fn run_once_tracking(
    renderer: eframe::Renderer,
    create: fn(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> (bool, eframe::Result) {
    let started = Rc::new(Cell::new(false));
    let flag = started.clone();
    let result = eframe::run_native(
        "岛仔",
        native_options(renderer),
        Box::new(move |cc| {
            // 应用闭包一旦被调用，说明图形上下文已经建立；之后的错误不再换渲染器。
            flag.set(true);
            Ok(create(cc))
        }),
    );
    (started.get(), result)
}

fn native_options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    let viewport = egui::ViewportBuilder::default()
        .with_title("岛仔")
        .with_icon(daozai::theme::app_icon())
        .with_inner_size([daozai::app::CANVAS_W, daozai::app::WIN_H])
        .with_decorations(false)
        .with_transparent(true)
        .with_active(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(false);
    // 同时启用 glow 与 wgpu 时，eframe 的默认渲染器是 wgpu。这里显式指定，避免有 OpenGL 的机器被静默换掉。
    let mut options = eframe::NativeOptions {
        viewport,
        vsync: !cfg!(windows),
        renderer,
        ..Default::default()
    };
    if renderer == eframe::Renderer::Wgpu {
        configure_wgpu(&mut options);
    }
    options
}

fn configure_wgpu(options: &mut eframe::NativeOptions) {
    #[cfg(windows)]
    configure_windows_dx12(options);
    #[cfg(not(windows))]
    let _ = options;
}

#[cfg(windows)]
fn configure_windows_dx12(options: &mut eframe::NativeOptions) {
    let eframe::egui_wgpu::WgpuSetup::CreateNew(mut setup) = options.wgpu_options.wgpu_setup.clone() else {
        return;
    };
    // 用户已用 WGPU_BACKEND 指定后端时保留该选择；否则只用 DirectX 12，避开微软 OpenGL 1.1。
    if std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12;
        setup.native_adapter_selector = Some(std::sync::Arc::new(select_dx12_adapter));
    }
    options.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(setup);
}

#[cfg(windows)]
fn select_dx12_adapter(
    adapters: &[eframe::wgpu::Adapter],
    surface: Option<&eframe::wgpu::Surface<'_>>,
) -> Result<eframe::wgpu::Adapter, String> {
    let mut compatible: Vec<&eframe::wgpu::Adapter> = adapters.iter().filter(|adapter| surface_compatible(adapter, surface)).collect();
    if compatible.is_empty() {
        return Err(format!("没有兼容当前窗口的 DirectX 12 适配器。枚举到：{}", describe_adapters(adapters)));
    }
    compatible.sort_by_key(|adapter| render_mode::gpu_preference(gpu_class(adapter.get_info().device_type)));
    let chosen = compatible[0];
    let transparent = surface.map(|surface| {
        let modes = surface.get_capabilities(chosen).alpha_modes;
        modes.iter().any(|mode| {
            matches!(mode, eframe::wgpu::CompositeAlphaMode::PreMultiplied | eframe::wgpu::CompositeAlphaMode::PostMultiplied)
        })
    });
    render_mode::note_transparency(transparent);
    Ok(chosen.clone())
}

#[cfg(windows)]
fn surface_compatible(adapter: &eframe::wgpu::Adapter, surface: Option<&eframe::wgpu::Surface<'_>>) -> bool {
    match surface {
        Some(surface) => !surface.get_capabilities(adapter).formats.is_empty(),
        None => true,
    }
}

#[cfg(windows)]
fn gpu_class(kind: eframe::wgpu::DeviceType) -> render_mode::GpuClass {
    match kind {
        eframe::wgpu::DeviceType::DiscreteGpu => render_mode::GpuClass::Discrete,
        eframe::wgpu::DeviceType::IntegratedGpu => render_mode::GpuClass::Integrated,
        eframe::wgpu::DeviceType::VirtualGpu => render_mode::GpuClass::Virtual,
        eframe::wgpu::DeviceType::Cpu => render_mode::GpuClass::Cpu,
        _ => render_mode::GpuClass::Other,
    }
}

#[cfg(windows)]
fn describe_adapters(adapters: &[eframe::wgpu::Adapter]) -> String {
    if adapters.is_empty() {
        return "（无）".into();
    }
    adapters.iter().map(|adapter| {
        let info = adapter.get_info();
        format!("{} / {:?} / {:?}", info.name, info.device_type, info.backend)
    }).collect::<Vec<_>>().join("；")
}

fn graphics_context_failed(error: &eframe::Error) -> bool {
    matches!(
        error,
        eframe::Error::OpenGL(_)
            | eframe::Error::Glutin(_)
            | eframe::Error::NoGlutinConfigs(_, _)
            | eframe::Error::Winit(_)
            | eframe::Error::WinitEventLoop(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opengl_version_failure_triggers_fallback() {
        let error = eframe::Error::OpenGL("egui_glow requires opengl 2.0+".to_owned().into());
        assert!(graphics_context_failed(&error));
    }

    #[test]
    fn application_errors_do_not_switch_renderer() {
        let error = eframe::Error::AppCreation("界面初始化失败".into());
        assert!(!graphics_context_failed(&error));
    }
}
