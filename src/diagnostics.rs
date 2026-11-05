//! 启动错误与 Rust 异常保存在本机，不自动上传账号或应用数据。

use std::path::{Path, PathBuf};

fn write_report(directory: &Path, title: &str, details: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(directory)?;
    let path = directory.join("last-error.log");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    std::fs::write(&path, format!(
        "岛仔 {}\n系统：{} / {}\n时间（Unix 秒）：{timestamp}\n{title}\n{details}\n",
        env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH,
    ))?;
    Ok(path)
}

fn message(directory: &Path, title: &str, details: &str) -> String {
    match write_report(directory, title, details) {
        Ok(path) => format!("{title}\n\n请将以下日志发给开发者，帮助定位问题：\n{}", path.display()),
        Err(error) => format!("{title}\n\n{details}\n\n日志无法保存：{error}\n请截图反馈。"),
    }
}

pub fn failure_log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("daozai-last-error.txt")
}

pub fn failure_text(title: &str, details: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    format!(
        "\u{feff}岛仔 {}\n系统：{} / {}\n时间（Unix 秒）：{timestamp}\n{title}\n{details}\n",
        env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH,
    )
}

pub fn write_failure_at(path: &Path, title: &str, details: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, failure_text(title, details))
}

/// 无控制台的 Windows 程序退出前把错误留在临时目录，并尽量写到父控制台。
pub fn record_failure(title: &str, details: &str) -> PathBuf {
    let path = failure_log_path();
    if let Err(error) = write_failure_at(&path, title, details) {
        note(&format!("{title}\n{details}\n临时日志无法保存：{error}"));
    } else {
        note(&format!("{title}\n{details}\n临时日志：{}", path.display()));
    }
    path
}

pub fn clear_recorded_failure() {
    let _ = std::fs::remove_file(failure_log_path());
}

pub fn note(text: &str) {
    eprintln!("{text}");
    #[cfg(windows)]
    write_parent_console(text);
}

#[cfg(windows)]
fn write_parent_console(text: &str) {
    use std::io::Write;
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe { let _ = AttachConsole(ATTACH_PARENT_PROCESS); }
    if let Ok(mut console) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
        let _ = writeln!(console, "{text}");
    }
}

pub fn report(title: &str, details: &str) {
    let temp = record_failure(title, details);
    let mut text = message(&daozai::model::config_dir().join("diagnostics"), title, details);
    text.push_str(&format!("\n\n若找不到上面的文件，请改看：\n{}", temp.display()));
    #[cfg(windows)]
    {
        use windows::core::{w, PCWSTR};
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        unsafe { MessageBoxW(None, PCWSTR(wide.as_ptr()), w!("岛仔"), MB_OK | MB_ICONERROR); }
    }
    #[cfg(not(windows))]
    eprintln!("{text}");
}

/// 仅记录最近一次启动停留的阶段，不记录账号或用户内容。
pub fn startup(stage:&str) {
    let directory=daozai::model::config_dir().join("diagnostics");
    if std::fs::create_dir_all(&directory).is_ok() {
        let _=std::fs::write(directory.join("last-startup.log"),format!(
            "岛仔 {}\n进程：{}\n启动阶段：{stage}\n",env!("CARGO_PKG_VERSION"),std::process::id()));
    }
}

/// 记录实际使用的图形设备，避免把电脑里的显卡列表误当作当前渲染设备。
pub fn graphics(cc:&eframe::CreationContext<'_>) {
    let active = describe_graphics(cc);
    let directory=daozai::model::config_dir().join("diagnostics");
    if std::fs::create_dir_all(&directory).is_ok(){let _=std::fs::write(directory.join("last-graphics.log"),&active.detail);}
    let name = active.name;
    daozai::render_mode::set_active(active);
    if cc.gl.is_some() || cc.wgpu_render_state.is_some() {
        clear_recorded_failure();
    }
    startup(&format!("图形设备就绪：{name}"));
}

fn describe_graphics(cc:&eframe::CreationContext<'_>) -> daozai::render_mode::ActiveRender {
    if let Some(gl)=&cc.gl {
        use eframe::glow::HasContext;
        let detail=unsafe{format!("后端：glow\n厂商：{}\n渲染器：{}\nOpenGL：{}\n",
            gl.get_parameter_string(eframe::glow::VENDOR),gl.get_parameter_string(eframe::glow::RENDERER),gl.get_parameter_string(eframe::glow::VERSION))};
        return daozai::render_mode::ActiveRender { name: "glow", detail, transparent: Some(true) };
    }
    if let Some(state)=&cc.wgpu_render_state {
        let info=state.adapter.get_info();
        let detail=format!(
            "后端：wgpu / {:?}\n设备：{}\n类型：{:?}\n驱动：{} {}\n透明合成：{}\n",
            info.backend, info.name, info.device_type, info.driver, info.driver_info,
            match daozai::render_mode::noted_transparency() {
                Some(true) => "支持",
                Some(false) => "不支持",
                None => "未知",
            },
        );
        return daozai::render_mode::ActiveRender {
            name: "wgpu",
            detail,
            transparent: daozai::render_mode::noted_transparency(),
        };
    }
    daozai::render_mode::ActiveRender { name: "unknown", detail: "未获得图形设备\n".into(), transparent: None }
}

pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        // 不写入 panic 的数据载荷，避免把邮件、凭据或登录信息带进日志。
        let location = info.location().map(|p| format!("{}:{}:{}", p.file(), p.line(), p.column()))
            .unwrap_or_else(|| "未知".into());
        report("岛仔发生异常，请重新启动。", &format!(
            "代码位置：{location}\n调用栈：\n{}", std::backtrace::Backtrace::force_capture(),
        ));
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("daozai-diagnostics-{}-{name}", std::process::id()))
    }

    #[test]
    fn failure_creates_a_report_and_exposes_its_location() {
        let directory = directory("report");
        let text = message(&directory, "启动失败", "OpenGL 初始化失败");
        let path = directory.join("last-error.log");
        assert!(text.contains(&path.display().to_string()));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
        assert!(content.contains("OpenGL 初始化失败"));
        // 重复失败只保留最后一份，避免常驻程序无限累积日志。
        write_report(&directory, "第二次失败", "新的错误").unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("OpenGL"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn unwritable_log_keeps_the_error_visible() {
        let path = directory("blocked");
        std::fs::write(&path, b"test").unwrap();
        let text = message(&path, "启动失败", "OpenGL 初始化失败");
        assert!(text.contains("OpenGL 初始化失败"));
        assert!(text.contains("日志无法保存"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn temp_failure_file_keeps_the_opengl_error_readable() {
        let path = directory("temp-error").join("daozai-last-error.txt");
        write_failure_at(&path, "OpenGL 不可用，正在尝试兼容渲染（DirectX / wgpu）。", "egui_glow requires opengl 2.0+").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with('\u{feff}'));
        assert!(content.contains("egui_glow requires opengl 2.0+"));
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
