use codex_context_manager::config::{Preset, Snapshot, display_path, format_tokens};
use eframe::egui::{self, Color32, FontId, RichText, Stroke, Vec2};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

const WINDOW_SIZE: Vec2 = Vec2::new(360.0, 190.0);
const MUTED: Color32 = Color32::from_gray(100);

pub fn launch(path: PathBuf) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Codex 上下文")
            .with_inner_size(WINDOW_SIZE)
            .with_resizable(false)
            .with_icon(egui::IconData {
                rgba: crate::app_icon::rgba(64),
                width: 64,
                height: 64,
            }),
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Codex Context Manager",
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            Ok(Box::new(Manager::new(path)))
        }),
    )
}

fn configure_style(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_paths = if cfg!(windows) {
        let system = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:/Windows"));
        vec![
            system.join("Fonts/msyh.ttc"),
            system.join("Fonts/simhei.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        ]
    };
    for path in font_paths {
        if let Ok(data) = fs::read(path) {
            fonts.font_data.insert(
                "chinese".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("chinese".to_owned());
            break;
        }
    }
    ctx.set_fonts(fonts);
    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.override_text_color = Some(Color32::BLACK);
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.selection.bg_fill = Color32::BLACK;
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(205));
    style.visuals.widgets.hovered.bg_fill = Color32::from_gray(238);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::BLACK);
    style.spacing.item_spacing = Vec2::new(8.0, 0.0);
    style.spacing.button_padding = Vec2::new(8.0, 6.0);
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    ctx.set_global_style(style);
}

struct Notice {
    text: String,
    details: Option<String>,
}

struct Manager {
    path: PathBuf,
    snapshot: Option<Snapshot>,
    selected: Option<Preset>,
    load_error: Option<String>,
    notice: Option<Notice>,
    changed_externally: bool,
    last_poll: Instant,
}

impl Manager {
    fn new(path: PathBuf) -> Self {
        let mut app = Self {
            path,
            snapshot: None,
            selected: None,
            load_error: None,
            notice: None,
            changed_externally: false,
            last_poll: Instant::now(),
        };
        app.reload();
        app
    }

    fn reload(&mut self) {
        self.notice = None;
        self.changed_externally = false;
        match Snapshot::read(&self.path) {
            Ok(snapshot) => {
                self.selected = snapshot.matching_preset();
                self.path = snapshot.path.clone();
                self.snapshot = Some(snapshot);
                self.load_error = None;
            }
            Err(error) => {
                self.snapshot = None;
                self.selected = None;
                self.load_error = Some(error.to_string());
            }
        }
    }

    fn apply(&mut self) {
        let (Some(snapshot), Some(preset)) = (&self.snapshot, self.selected) else {
            return;
        };
        match snapshot.apply(preset) {
            Ok(change) => {
                self.selected = change.snapshot.matching_preset();
                self.snapshot = Some(change.snapshot);
                self.changed_externally = false;
                self.notice = Some(Notice {
                    text: format!(
                        "已应用 {}。重启 Codex 并新开对话。",
                        format_tokens(preset.window())
                    ),
                    details: None,
                });
            }
            Err(error) => {
                self.notice = Some(Notice {
                    text: "修改失败，悬停查看详情；按 F5 刷新。".to_owned(),
                    details: Some(error.to_string()),
                })
            }
        }
    }

    fn can_apply(&self) -> bool {
        !self.changed_externally
            && self.selected.is_some()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.matching_preset() != self.selected)
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if self.last_poll.elapsed() >= Duration::from_secs(2) {
            self.last_poll = Instant::now();
            if let Some(snapshot) = &self.snapshot {
                self.changed_externally = fs::read(&snapshot.path)
                    .map(|bytes| bytes != snapshot.bytes)
                    .unwrap_or(true);
            }
        }
        ctx.request_repaint_after(Duration::from_secs(2));
    }

    fn pick_config(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Codex 配置", &["toml"])
            .set_directory(
                self.path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
            )
            .pick_file()
        {
            self.path = path;
            self.reload();
        }
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        for (key, preset) in [
            (egui::Key::Num1, Preset::Default),
            (egui::Key::Num2, Preset::High),
            (egui::Key::Num3, Preset::Max),
        ] {
            if ctx.input(|input| input.key_pressed(key)) {
                self.selected = Some(preset);
            }
        }
        if ctx.input(|input| input.key_pressed(egui::Key::F5)) {
            self.reload();
        }
        if ctx.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter))
            && self.can_apply()
        {
            self.apply();
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::WHITE).inner_margin(18.0))
            .show(ui, |ui| {
                let current = self
                    .snapshot
                    .as_ref()
                    .map(|snapshot| {
                        snapshot
                            .window()
                            .map(format_tokens)
                            .unwrap_or_else(|| "跟随模型".to_owned())
                    })
                    .unwrap_or_else(|| "无法读取配置".to_owned());
                ui.label(RichText::new(format!("当前上下文：{current}")).size(16.0))
                    .on_hover_text(display_path(&self.path));
                ui.add_space(14.0);
                let option_width = (ui.available_width() - 16.0) / 3.0;
                ui.horizontal(|ui| {
                    for (preset, label) in [
                        (Preset::Default, "默认 272K"),
                        (Preset::High, "500K"),
                        (Preset::Max, "1M"),
                    ] {
                        let selected = self.selected == Some(preset);
                        let response = ui.add_sized(
                            [option_width, 36.0],
                            egui::Button::new(RichText::new(label).color(if selected {
                                Color32::WHITE
                            } else {
                                Color32::BLACK
                            }))
                            .fill(if selected {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if selected {
                                    Color32::BLACK
                                } else {
                                    Color32::from_gray(205)
                                },
                            )),
                        );
                        if response.clicked() {
                            self.selected = Some(preset);
                            ctx.request_repaint();
                        }
                    }
                });
                ui.add_space(12.0);
                if self.snapshot.is_none() {
                    if ui
                        .add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("选择配置文件…"),
                        )
                        .clicked()
                    {
                        self.pick_config();
                    }
                } else {
                    let label = if self.selected.is_some()
                        && self
                            .snapshot
                            .as_ref()
                            .is_some_and(|s| s.matching_preset() == self.selected)
                    {
                        "已应用"
                    } else {
                        "应用"
                    };
                    if ui
                        .add_enabled_ui(self.can_apply(), |ui| {
                            ui.add_sized(
                                [ui.available_width(), 32.0],
                                egui::Button::new(RichText::new(label).color(Color32::WHITE))
                                    .fill(Color32::BLACK),
                            )
                        })
                        .inner
                        .clicked()
                    {
                        self.apply();
                    }
                }
                ui.add_space(10.0);
                let (status, details) = if self.changed_externally {
                    ("配置已变化，按 F5 刷新后再修改。", None)
                } else if let Some(error) = &self.load_error {
                    ("请选择 Codex 的 config.toml 文件。", Some(error.as_str()))
                } else if let Some(notice) = &self.notice {
                    (notice.text.as_str(), notice.details.as_deref())
                } else {
                    ("修改后重启 Codex，并新开对话。", None)
                };
                let response = ui.label(RichText::new(status).size(12.0).color(MUTED));
                if let Some(details) = details {
                    response.on_hover_text(details);
                }
            });
    }
}

impl eframe::App for Manager {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(
        app: &mut Manager,
        ctx: &egui::Context,
        mut events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
    ) {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW_SIZE)),
            events,
            focused: true,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
    }

    fn click(app: &mut Manager, ctx: &egui::Context, x: f32, y: f32) {
        let pos = egui::pos2(x, y);
        for pressed in [true, false] {
            draw(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        draw(app, ctx, vec![], egui::Modifiers::NONE);
    }

    fn key(app: &mut Manager, ctx: &egui::Context, key: egui::Key, modifiers: egui::Modifiers) {
        for pressed in [true, false] {
            draw(
                app,
                ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
            );
        }
    }

    #[test]
    fn compact_ui_switches_three_presets_with_mouse_and_keyboard() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("config.toml");
        fs::write(&path, "model = \"gpt-6.1-sol\"\nmodel_context_window = 500000\nmodel_auto_compact_token_limit = 450000\n[desktop]\ntheme = \"dark\"\n").unwrap();
        let mut app = Manager::new(path.clone());
        let ctx = egui::Context::default();
        configure_style(&ctx);
        draw(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        draw(&mut app, &ctx, vec![], egui::Modifiers::NONE);
        click(&mut app, &ctx, 69.0, 70.0);
        assert_eq!(app.selected, Some(Preset::Default));
        assert_eq!(
            Snapshot::read(&path).unwrap().window(),
            Some(500000),
            "selecting alone must not write"
        );
        click(&mut app, &ctx, 180.0, 116.0);
        assert_eq!(Snapshot::read(&path).unwrap().window(), Some(272000));
        key(&mut app, &ctx, egui::Key::Num2, egui::Modifiers::NONE);
        key(&mut app, &ctx, egui::Key::Enter, egui::Modifiers::CTRL);
        assert_eq!(Snapshot::read(&path).unwrap().window(), Some(500000));
        click(&mut app, &ctx, 289.0, 70.0);
        click(&mut app, &ctx, 180.0, 116.0);
        let after = Snapshot::read(&path).unwrap();
        assert_eq!(after.window(), Some(1000000));
        assert_eq!(after.doc["desktop"]["theme"].as_str(), Some("dark"));
        assert_eq!(fs::read_dir(after.backup_dir()).unwrap().count(), 3);
    }
}
