//! file-seal GUI — desktop product with dashboard, drag-drop, batch ops.

use eframe::egui::{self, Color32, RichText};
use file_seal::{self, SealError, SealRecord};
use std::path::{Path, PathBuf};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1100.0, 720.0)),
        min_window_size: Some(egui::vec2(800.0, 520.0)),
        drag_and_drop_support: true,
        ..Default::default()
    };
    eframe::run_native(
        "File Seal — 本地文件完整性鉴定",
        options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.window_rounding = 8.0.into();
            visuals.panel_fill = Color32::from_rgb(18, 18, 24);
            cc.egui_ctx.set_visuals(visuals);
            Box::new(FileSealApp::new(cc))
        }),
    )
}

fn file_icon(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" => "🖼️",
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "flv" | "wmv" => "🎬",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => "🎵",
        "pdf" => "📄",
        "doc" | "docx" | "txt" | "md" | "rtf" => "📝",
        "zip" | "rar" | "7z" | "tar" | "gz" => "📦",
        "exe" | "msi" | "dmg" | "app" => "⚙️",
        _ => "📎",
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Dashboard,
    Seal,
    Verify,
    History,
    Settings,
}

struct FileSealApp {
    tab: Tab,
    ledger_path: PathBuf,
    pending_paths: Vec<PathBuf>,
    seal_message: String,
    last_seal_results: Vec<String>,
    verify_paths: Vec<PathBuf>,
    last_verify_results: Vec<(bool, String)>,
    records: Vec<SealRecord>,
    history_filter: String,
    status: String,
    error: Option<String>,
    total_seals: usize,
    unique_paths: usize,
}

impl FileSealApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let ledger =
            file_seal::resolve_ledger(None).unwrap_or_else(|_| PathBuf::from("seals.jsonl"));
        let mut app = Self {
            tab: Tab::Dashboard,
            ledger_path: ledger,
            pending_paths: Vec::new(),
            seal_message: String::new(),
            last_seal_results: Vec::new(),
            verify_paths: Vec::new(),
            last_verify_results: Vec::new(),
            records: Vec::new(),
            history_filter: String::new(),
            status: "就绪 — 可拖放文件到窗口".into(),
            error: None,
            total_seals: 0,
            unique_paths: 0,
        };
        app.reload_history();
        app
    }

    fn reload_history(&mut self) {
        match file_seal::read_all(&self.ledger_path) {
            Ok(mut recs) => {
                recs.reverse();
                self.total_seals = recs.len();
                let mut paths = std::collections::HashSet::new();
                for r in &recs {
                    paths.insert(r.path.clone());
                }
                self.unique_paths = paths.len();
                self.records = recs;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!("读取账本失败: {e}"));
                self.records.clear();
                self.total_seals = 0;
                self.unique_paths = 0;
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        match self.tab {
            Tab::Seal | Tab::Dashboard => {
                for p in dropped {
                    if p.is_file() && !self.pending_paths.contains(&p) {
                        self.pending_paths.push(p);
                    }
                }
                self.tab = Tab::Seal;
                self.status = format!("已拖入 {} 个文件，准备封印", self.pending_paths.len());
            }
            Tab::Verify => {
                for p in dropped {
                    if p.is_file() && !self.verify_paths.contains(&p) {
                        self.verify_paths.push(p);
                    }
                }
                self.status = format!("已拖入 {} 个文件，准备验证", self.verify_paths.len());
            }
            _ => {}
        }
    }

    fn do_seal_one(&mut self, path: &Path) -> String {
        match file_seal::seal_file(&self.ledger_path, path, {
            let m = self.seal_message.trim();
            if m.is_empty() {
                None
            } else {
                Some(m.to_string())
            }
        }) {
            Ok(rec) => {
                let name = Path::new(&rec.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&rec.path);
                format!(
                    "✅ {} {}  ·  {}  ·  {} B",
                    file_icon(path),
                    name,
                    file_seal::short_hash(&rec.sha256),
                    rec.size
                )
            }
            Err(e) => format!("❌ {} 失败: {e}", path.display()),
        }
    }

    fn do_verify_one(&mut self, path: &Path) -> (bool, String) {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
        match file_seal::verify_file(&self.ledger_path, path) {
            Ok(prev) => (
                true,
                format!(
                    "✅ {} {}  与 {} 一致  ·  {}",
                    file_icon(path),
                    name,
                    prev.ts.format("%Y-%m-%d %H:%M"),
                    file_seal::short_hash(&prev.sha256)
                ),
            ),
            Err(SealError::NoSeal(_)) => (
                false,
                format!("⚠️ {} {}  未找到封印，请先封印", file_icon(path), name),
            ),
            Err(SealError::HashMismatch {
                recorded, current, ..
            }) => (
                false,
                format!(
                    "❌ {} {}  已被修改  记录:{}  当前:{}",
                    file_icon(path),
                    name,
                    recorded,
                    current
                ),
            ),
            Err(e) => (false, format!("❌ {} 错误: {e}", name)),
        }
    }
}

impl eframe::App for FileSealApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ctx);

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("File Seal").strong().size(22.0));
                ui.label(RichText::new("本地文件完整性鉴定").weak().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(&self.status)
                            .small()
                            .color(Color32::from_rgb(140, 180, 255)),
                    );
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Dashboard, "📊 仪表盘");
                ui.selectable_value(&mut self.tab, Tab::Seal, "🔒 封印文件");
                ui.selectable_value(&mut self.tab, Tab::Verify, "✅ 验证文件");
                ui.selectable_value(&mut self.tab, Tab::History, "📜 历史记录");
                ui.selectable_value(&mut self.tab, Tab::Settings, "⚙️ 设置");
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("账本: {}", self.ledger_path.display()));
                ui.separator();
                ui.label(format!(
                    "共 {} 条封印 · {} 个文件",
                    self.total_seals, self.unique_paths
                ));
                if let Some(ref e) = self.error {
                    ui.colored_label(Color32::from_rgb(255, 120, 120), e);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Dashboard => self.ui_dashboard(ui),
            Tab::Seal => self.ui_seal(ui),
            Tab::Verify => self.ui_verify(ui),
            Tab::History => self.ui_history(ui),
            Tab::Settings => self.ui_settings(ui),
        });
    }
}

impl FileSealApp {
    fn ui_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("仪表盘");
        ui.add_space(8.0);
        ui.label(
            RichText::new("提示：可直接把文件拖放到窗口，自动进入封印页面")
                .weak()
                .small(),
        );
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            self.stat_card(
                ui,
                "总封印数",
                &self.total_seals.to_string(),
                Color32::from_rgb(80, 160, 255),
            );
            ui.add_space(16.0);
            self.stat_card(
                ui,
                "独立文件",
                &self.unique_paths.to_string(),
                Color32::from_rgb(100, 200, 140),
            );
            ui.add_space(16.0);
            let ledger_exists = self.ledger_path.exists();
            self.stat_card(
                ui,
                "账本状态",
                if ledger_exists { "正常" } else { "新建" },
                if ledger_exists {
                    Color32::from_rgb(100, 200, 140)
                } else {
                    Color32::from_rgb(200, 180, 80)
                },
            );
        });

        ui.add_space(24.0);
        ui.heading("最近封印");
        ui.add_space(8.0);

        if self.records.is_empty() {
            ui.label(RichText::new("暂无封印记录。去「封印文件」页面或直接拖放文件进来。").weak());
        } else {
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    for rec in self.records.iter().take(15) {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(rec.ts.format("%m-%d %H:%M").to_string())
                                        .monospace()
                                        .weak(),
                                );
                                ui.label(file_seal::short_hash(&rec.sha256));
                                let p = Path::new(&rec.path);
                                let icon = file_icon(p);
                                let name =
                                    p.file_name().and_then(|s| s.to_str()).unwrap_or(&rec.path);
                                ui.label(RichText::new(format!("{icon} {name}")).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(format!("{} B", rec.size));
                                    },
                                );
                            });
                            if let Some(ref m) = rec.message {
                                ui.label(RichText::new(format!("备注: {m}")).small().italics());
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
        }

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if ui.button("刷新数据").clicked() {
                self.reload_history();
                self.status = "已刷新".into();
            }
            if ui.button("去封印文件 →").clicked() {
                self.tab = Tab::Seal;
            }
        });
    }

    fn stat_card(&self, ui: &mut egui::Ui, title: &str, value: &str, accent: Color32) {
        egui::Frame::none()
            .fill(Color32::from_rgb(28, 28, 36))
            .rounding(8.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.set_min_width(140.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).small().weak());
                    ui.label(RichText::new(value).size(28.0).color(accent).strong());
                });
            });
    }

    fn ui_seal(&mut self, ui: &mut egui::Ui) {
        ui.heading("封印文件 / 图片 / 视频");
        ui.label(
            RichText::new("支持任意文件类型。可多选、选文件夹，或直接拖放文件到窗口。").weak(),
        );
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.button("📁 选择文件…").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("选择要封印的文件")
                    .pick_files()
                {
                    for p in paths {
                        if !self.pending_paths.contains(&p) {
                            self.pending_paths.push(p);
                        }
                    }
                    self.last_seal_results.clear();
                }
            }
            if ui.button("📂 选择文件夹…").clicked() {
                if let Some(folder) = rfd::FileDialog::new().set_title("选择文件夹").pick_folder()
                {
                    if let Ok(entries) = std::fs::read_dir(&folder) {
                        for e in entries.filter_map(|e| e.ok()) {
                            let p = e.path();
                            if p.is_file() && !self.pending_paths.contains(&p) {
                                self.pending_paths.push(p);
                            }
                        }
                        self.last_seal_results.clear();
                    }
                }
            }
            if !self.pending_paths.is_empty() && ui.button("清空列表").clicked() {
                self.pending_paths.clear();
                self.last_seal_results.clear();
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("可选备注（应用到本批所有文件）:");
            ui.text_edit_singleline(&mut self.seal_message);
        });

        ui.add_space(8.0);

        if !self.pending_paths.is_empty() {
            ui.group(|ui| {
                ui.label(format!(
                    "待封印 ({} 个) — 支持拖放继续添加:",
                    self.pending_paths.len()
                ));
                egui::ScrollArea::vertical()
                    .max_height(140.0)
                    .show(ui, |ui| {
                        for p in &self.pending_paths {
                            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                            ui.label(format!("{} {}  ({} bytes)", file_icon(p), name, size));
                        }
                    });
            });

            ui.add_space(8.0);
            if ui
                .button(
                    RichText::new(format!("🔒 开始封印 ({} 个)", self.pending_paths.len()))
                        .strong(),
                )
                .clicked()
            {
                let paths = self.pending_paths.clone();
                self.last_seal_results.clear();
                let mut ok = 0;
                for p in &paths {
                    let r = self.do_seal_one(p);
                    if r.starts_with('✅') {
                        ok += 1;
                    }
                    self.last_seal_results.push(r);
                }
                self.pending_paths.clear();
                self.reload_history();
                self.status = format!("本批完成：成功 {} / {}", ok, paths.len());
            }
        } else {
            ui.label(RichText::new("请选择文件，或把文件直接拖放到此窗口").weak());
        }

        if !self.last_seal_results.is_empty() {
            ui.add_space(16.0);
            ui.heading("本批结果");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for r in &self.last_seal_results {
                        ui.label(RichText::new(r).monospace().small());
                    }
                });
        }
    }

    fn ui_verify(&mut self, ui: &mut egui::Ui) {
        ui.heading("验证文件完整性");
        ui.label(RichText::new("检查文件是否与封印时完全一致。支持批量验证与拖放。").weak());
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            if ui.button("📁 选择要验证的文件…").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .set_title("选择要验证的文件")
                    .pick_files()
                {
                    for p in paths {
                        if !self.verify_paths.contains(&p) {
                            self.verify_paths.push(p);
                        }
                    }
                    self.last_verify_results.clear();
                }
            }
            if !self.verify_paths.is_empty() && ui.button("清空列表").clicked() {
                self.verify_paths.clear();
                self.last_verify_results.clear();
            }
        });

        if !self.verify_paths.is_empty() {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(format!("待验证 ({} 个):", self.verify_paths.len()));
                for p in &self.verify_paths {
                    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("?");
                    ui.label(format!("{} {}", file_icon(p), name));
                }
            });

            ui.add_space(8.0);
            if ui
                .button(
                    RichText::new(format!("✅ 立即验证 ({} 个)", self.verify_paths.len())).strong(),
                )
                .clicked()
            {
                let paths = self.verify_paths.clone();
                self.last_verify_results.clear();
                let mut ok = 0;
                for p in &paths {
                    let (pass, msg) = self.do_verify_one(p);
                    if pass {
                        ok += 1;
                    }
                    self.last_verify_results.push((pass, msg));
                }
                self.status = format!("验证完成：通过 {} / {}", ok, paths.len());
            }
        } else {
            ui.label(RichText::new("请选择文件，或拖放文件到此窗口").weak());
        }

        if !self.last_verify_results.is_empty() {
            ui.add_space(16.0);
            let pass_count = self
                .last_verify_results
                .iter()
                .filter(|(ok, _)| *ok)
                .count();
            let total = self.last_verify_results.len();
            ui.heading(format!("本批结果：通过 {} / {}", pass_count, total));
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for (ok, msg) in &self.last_verify_results {
                        let color = if *ok {
                            Color32::from_rgb(100, 220, 140)
                        } else {
                            Color32::from_rgb(255, 140, 120)
                        };
                        ui.colored_label(color, RichText::new(msg).monospace().small());
                    }
                });
        }
    }

    fn ui_history(&mut self, ui: &mut egui::Ui) {
        ui.heading("历史记录");
        ui.horizontal(|ui| {
            ui.label("过滤:");
            ui.text_edit_singleline(&mut self.history_filter);
            if ui.button("刷新").clicked() {
                self.reload_history();
            }
        });
        ui.add_space(8.0);

        let filter = self.history_filter.to_lowercase();
        let filtered: Vec<_> = self
            .records
            .iter()
            .filter(|r| {
                filter.is_empty()
                    || r.path.to_lowercase().contains(&filter)
                    || r.message
                        .as_ref()
                        .map(|m| m.to_lowercase().contains(&filter))
                        .unwrap_or(false)
            })
            .collect();

        if filtered.is_empty() {
            ui.label("(无匹配记录)");
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("时间").strong().monospace());
                ui.add_space(70.0);
                ui.label(RichText::new("哈希").strong().monospace());
                ui.add_space(30.0);
                ui.label(RichText::new("大小").strong());
                ui.add_space(16.0);
                ui.label(RichText::new("文件").strong());
            });
            ui.separator();

            for rec in filtered.iter().take(300) {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(rec.ts.format("%Y-%m-%d %H:%M:%S").to_string())
                            .monospace()
                            .small(),
                    );
                    ui.label(
                        RichText::new(file_seal::short_hash(&rec.sha256))
                            .monospace()
                            .small(),
                    );
                    ui.label(RichText::new(format!("{:>8}", rec.size)).small());
                    let p = Path::new(&rec.path);
                    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or(&rec.path);
                    ui.label(RichText::new(format!("{} {}", file_icon(p), name)).small());
                });
                if let Some(ref m) = rec.message {
                    ui.label(
                        RichText::new(format!("    备注: {m}"))
                            .small()
                            .italics()
                            .weak(),
                    );
                }
            }
        });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.add_space(12.0);

        ui.label("账本保存路径（独立于文件本身，可任意指定）");
        ui.horizontal(|ui| {
            ui.monospace(self.ledger_path.display().to_string());
            if ui.button("更改路径…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("选择账本文件位置")
                    .set_file_name("seals.jsonl")
                    .save_file()
                {
                    self.ledger_path = path;
                    self.reload_history();
                    self.status = "账本路径已更新".into();
                }
            }
            if ui.button("恢复默认").clicked() {
                if let Ok(p) = file_seal::resolve_ledger(None) {
                    self.ledger_path = p;
                    self.reload_history();
                    self.status = "已恢复默认账本路径".into();
                }
            }
        });

        ui.add_space(16.0);
        ui.label(
            RichText::new(
                "说明：\n\
• 所有封印只记录在本地账本，文件内容永不上传。\n\
• 账本默认位置：系统数据目录 / file-seal / seals.jsonl\n\
• 可指定任何路径作为独立保存位置。\n\
• 验证时比对当前文件字节与账本中最新记录的 SHA-256。\n\
• 支持拖放文件到窗口进行封印或验证。",
            )
            .small(),
        );

        ui.add_space(24.0);
        ui.separator();
        ui.label(
            RichText::new("File Seal v0.2.0  ·  MIT License  ·  本地优先")
                .weak()
                .small(),
        );
        ui.label(
            RichText::new("支持文件 · 图片 · 视频 · 音频 · 文档 等任意类型")
                .weak()
                .small(),
        );
    }
}
