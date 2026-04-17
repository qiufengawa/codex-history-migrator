use std::path::Path;

use eframe::egui::{self, Color32, ComboBox, RichText, ScrollArea, TextEdit, Ui};
use rfd::FileDialog;

use crate::app::MigratorApp;
use crate::models::manage::{ArchivedFilter, HealthFilter, ManageHealth};
use crate::platform::{open_file_location_command, open_path_command, run_platform_command};

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    render_toolbar(ui, app);

    if let Some(title) = app.running_task_title() {
        ui.small(format!("{title}执行中，管理页操作暂时锁定。"));
    }

    ui.add_space(8.0);
    render_action_bar(ui, app);

    ui.add_space(8.0);
    render_content_layout(ui, app);

    render_confirmation_dialogs(ui.ctx(), app);
}

fn render_toolbar(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_enabled_ui(!app.is_busy(), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Codex 数据目录").strong());
            let input_width = (ui.available_width() - 160.0).max(260.0);
            ui.add_sized(
                [input_width, 28.0],
                TextEdit::singleline(&mut app.codex_home_input),
            );
            if ui.button("刷新并检查").clicked() {
                app.run_manage_refresh();
            }
        });
    });
}

fn render_content_layout(ui: &mut Ui, app: &mut MigratorApp) {
    let available_width = ui.available_width();
    let panel_height = ui.available_height().max(460.0);
    if available_width >= 1260.0 {
        let spacing = ui.spacing().item_spacing.x;
        let left_width = (available_width * 0.23).clamp(240.0, 320.0);
        let right_width = (available_width * 0.35).clamp(360.0, 520.0);
        let middle_width = (available_width - left_width - right_width - spacing * 2.0).max(360.0);

        ui.horizontal_top(|ui| {
            render_panel(ui, left_width, panel_height, |ui| render_filters(ui, app));
            render_panel(ui, middle_width, panel_height, |ui| {
                render_thread_list(ui, app)
            });
            render_panel(ui, right_width, panel_height, |ui| {
                render_detail_panel(ui, app)
            });
        });
    } else {
        render_panel(ui, available_width, 260.0, |ui| render_filters(ui, app));
        ui.add_space(6.0);
        render_panel(
            ui,
            available_width,
            (panel_height * 0.42).max(280.0),
            |ui| render_thread_list(ui, app),
        );
        ui.add_space(6.0);
        render_panel(
            ui,
            available_width,
            (panel_height * 0.48).max(320.0),
            |ui| render_detail_panel(ui, app),
        );
    }
}

fn render_panel(ui: &mut Ui, width: f32, min_height: f32, add_contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, min_height),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_min_height(min_height);
            add_contents(ui);
        },
    );
}

fn render_filters(ui: &mut Ui, app: &mut MigratorApp) {
    ui.heading("筛选");
    ui.add_space(4.0);

    ScrollArea::vertical()
        .id_salt("manage_filters")
        .show(ui, |ui| {
            ui.label("关键词");
            ui.add_sized(
                [ui.available_width(), 28.0],
                TextEdit::singleline(&mut app.manage_filter.keyword)
                    .hint_text("搜索标题、摘要、线程 ID、路径、Provider、模型"),
            );

            ui.add_space(10.0);
            ui.label("归档状态");
            ComboBox::from_id_salt("manage_archived_filter")
                .width(ui.available_width())
                .selected_text(archived_filter_label(app.manage_filter.archived))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut app.manage_filter.archived,
                        ArchivedFilter::All,
                        "全部",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.archived,
                        ArchivedFilter::ActiveOnly,
                        "仅未归档",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.archived,
                        ArchivedFilter::ArchivedOnly,
                        "仅已归档",
                    );
                });

            ui.add_space(10.0);
            ui.label("Provider");
            let selected_provider = app
                .manage_filter
                .provider
                .clone()
                .unwrap_or_else(|| "全部".to_string());
            ComboBox::from_id_salt("manage_provider_filter")
                .width(ui.available_width())
                .selected_text(selected_provider)
                .show_ui(ui, |ui| {
                    let is_all = app.manage_filter.provider.is_none();
                    if ui.selectable_label(is_all, "全部").clicked() {
                        app.manage_filter.provider = None;
                    }

                    for provider in app.manage_provider_options() {
                        let selected =
                            app.manage_filter.provider.as_deref() == Some(provider.as_str());
                        if ui.selectable_label(selected, &provider).clicked() {
                            app.manage_filter.provider = Some(provider);
                        }
                    }
                });

            ui.add_space(10.0);
            ui.label("数据健康状态");
            ComboBox::from_id_salt("manage_health_filter")
                .width(ui.available_width())
                .selected_text(health_filter_label(app.manage_filter.health))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut app.manage_filter.health, HealthFilter::All, "全部");
                    ui.selectable_value(
                        &mut app.manage_filter.health,
                        HealthFilter::HealthyOnly,
                        "正常",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.health,
                        HealthFilter::NeedsAttentionOnly,
                        "需处理",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.health,
                        HealthFilter::MissingPayloadOnly,
                        "缺失 Payload",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.health,
                        HealthFilter::InvalidPathOnly,
                        "异常路径",
                    );
                    ui.selectable_value(
                        &mut app.manage_filter.health,
                        HealthFilter::ArchiveStateMismatchOnly,
                        "归档目录不一致",
                    );
                });

            ui.add_space(12.0);
            if ui
                .add_sized([ui.available_width(), 30.0], egui::Button::new("清空筛选"))
                .clicked()
            {
                app.manage_filter.keyword.clear();
                app.manage_filter.archived = ArchivedFilter::All;
                app.manage_filter.provider = None;
                app.manage_filter.health = HealthFilter::All;
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(format!("已加载会话：{}", app.manage_rows.len()));
            ui.label(format!("回收站批次：{}", app.manage_trash_batches.len()));
        });
}

fn render_thread_list(ui: &mut Ui, app: &mut MigratorApp) {
    let rows = app.manage_filtered_rows();
    ui.horizontal(|ui| {
        ui.heading("会话列表");
        ui.label(format!("({})", rows.len()));
    });
    ui.small("支持多选、快速预览和批量操作。");
    ui.add_space(6.0);

    if rows.is_empty() {
        ui.label("暂无数据，点击上方“刷新管理视图”开始加载。");
        return;
    }

    ScrollArea::vertical()
        .id_salt("manage_rows")
        .show(ui, |ui| {
            for row in rows {
                let row_id = row.id.clone();
                let checked = app.manage_selected_ids.contains(&row_id);
                let focused = app.manage_detail_thread_id.as_deref() == Some(row_id.as_str());
                let expanded = focused || checked;
                let fill = if focused {
                    ui.visuals().selection.bg_fill.linear_multiply(0.28)
                } else if checked {
                    ui.visuals().selection.bg_fill.linear_multiply(0.10)
                } else {
                    Color32::TRANSPARENT
                };
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .fill(fill)
                    .stroke(egui::Stroke::NONE)
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            let mut next_checked = checked;
                            if ui.checkbox(&mut next_checked, "").changed() {
                                app.toggle_manage_selection(row_id.clone(), next_checked);
                            }

                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    let title_response = ui.add(
                                        egui::Label::new(
                                            RichText::new(clip_text(&row.title_display, 52))
                                                .strong()
                                                .size(15.0),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if title_response.clicked() {
                                        app.select_only_manage_row(row_id.clone());
                                    }
                                    title_response.on_hover_text(&row.title_display);

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let meta = format!(
                                                "{} · {}",
                                                row.model_provider,
                                                row.model
                                                    .clone()
                                                    .unwrap_or_else(|| "未标记模型".to_string()),
                                            );
                                            ui.label(
                                                RichText::new(meta)
                                                    .small()
                                                    .monospace()
                                                    .color(ui.visuals().weak_text_color()),
                                            );
                                        },
                                    );
                                });

                                ui.horizontal(|ui| {
                                    health_badge(ui, row.health);
                                    if !row.payload_exists {
                                        soft_badge(ui, "Payload 缺失");
                                    } else if row.archived {
                                        soft_badge(ui, "已归档");
                                    }
                                });

                                if expanded {
                                    if let Some(detail) = &row.title_detail {
                                        ui.add_space(2.0);
                                        ui.label(
                                            RichText::new(clip_text(detail, 88))
                                                .small()
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                    if !row.first_user_message_display.is_empty() {
                                        let response = ui.add(
                                            egui::Label::new(
                                                RichText::new(clip_text(
                                                    &row.first_user_message_display,
                                                    88,
                                                ))
                                                .small()
                                                .color(ui.visuals().weak_text_color()),
                                            )
                                            .wrap(),
                                        );
                                        response.on_hover_text(&row.first_user_message_display);
                                    }
                                    if row.title != row.title_display
                                        || (row.archived && row.payload_exists)
                                    {
                                        ui.horizontal_wrapped(|ui| {
                                            if row.title != row.title_display {
                                                soft_badge(ui, "已整理");
                                            }
                                            if row.archived && row.payload_exists {
                                                soft_badge(ui, "已归档");
                                            }
                                        });
                                    }
                                }
                            });
                        });
                    });
                ui.add_space(2.0);
            }
        });
}

fn render_detail_panel(ui: &mut Ui, app: &mut MigratorApp) {
    ui.heading("详情");
    let Some(row) = app.manage_detail_row().cloned() else {
        ui.label("选择一条会话后，这里会显示元数据和预览。");
        ui.add_space(10.0);
        render_trash_panel(ui, app);
        return;
    };

    ScrollArea::vertical()
        .id_salt("manage_detail")
        .show(ui, |ui| {
            ui.group(|ui| {
                ui.label(RichText::new(&row.title_display).heading().strong());
                if let Some(detail) = &row.title_detail {
                    ui.small(detail);
                }
                if !row.first_user_message_display.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("摘要")
                                .small()
                                .strong()
                                .color(ui.visuals().weak_text_color()),
                        );
                        let response = ui.add(
                            egui::Label::new(clip_text(&row.first_user_message_display, 88)).wrap(),
                        );
                        response.on_hover_text(&row.first_user_message_display);
                    });
                }
            });

            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("会话信息").strong());
                ui.add_space(6.0);
                render_metadata_grid(ui, app, &row);
            });

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(RichText::new("操作").strong());
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let input_width = (ui.available_width() - 110.0).max(220.0);
                    ui.add_sized(
                        [input_width, 28.0],
                        TextEdit::singleline(&mut app.manage_rename_input),
                    );
                    if ui
                        .add_enabled(
                            app.manage_selected_ids.len() <= 1,
                            egui::Button::new("保存标题"),
                        )
                        .clicked()
                    {
                        app.run_manage_rename();
                    }
                });

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(row.can_open_payload, egui::Button::new("打开会话文件"))
                        .clicked()
                    {
                        if let Err(error) = open_path(&row.rollout_path) {
                            app.log(format!("打开会话文件失败：{error}"));
                        }
                    }

                    if ui.button("打开文件位置").clicked() {
                        if let Err(error) = open_file_location(&row.rollout_path) {
                            app.log(format!("打开文件位置失败：{error}"));
                        }
                    }
                });

                ui.add_space(8.0);
                if ui
                    .add_enabled(
                        !app.is_busy(),
                        egui::Button::new(
                            RichText::new("删除当前会话到工具回收站")
                                .strong()
                                .color(Color32::from_rgb(171, 51, 51)),
                        ),
                    )
                    .clicked()
                {
                    app.request_manage_delete_for_detail();
                }
                ui.small("删除不会直接永久清空，而是先进入工具回收站。");
            });

            ui.add_space(10.0);
            ui.group(|ui| {
                ui.label(RichText::new("最近正文预览").strong());
                ui.small("已优先显示可读摘要，无法识别时再回退为原始内容。");
                ui.add_space(6.0);

                if app.manage_preview_entries.is_empty() {
                    ui.label("当前会话暂无可预览内容。");
                } else {
                    for entry in &app.manage_preview_entries {
                        egui::Frame::group(ui.style())
                            .fill(ui.visuals().faint_bg_color)
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new(format!("#{}", entry.line_number))
                                            .monospace()
                                            .small(),
                                    );
                                    soft_badge(ui, &entry.display_type);
                                    if entry.is_fallback {
                                        ui.label(
                                            RichText::new("容错摘要")
                                                .small()
                                                .color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(entry.text.as_str())
                                        .wrap()
                                        .selectable(true),
                                );
                            });
                        ui.add_space(4.0);
                    }
                }
            });

            ui.add_space(10.0);
            render_trash_panel(ui, app);
        });
}

fn render_trash_panel(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_space(10.0);
    ui.separator();
    ui.heading("回收站");

    if app.manage_trash_batches.is_empty() {
        ui.label("当前没有回收站批次。");
        return;
    }

    let batches = app.manage_trash_batches.clone();
    for batch in batches {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&batch.batch_id);
                ui.label(format!(
                    "{} 条会话 / {} 个 payload",
                    batch.thread_count, batch.payload_count
                ));
            });
            ui.small(batch.path.to_string_lossy());
            ui.horizontal(|ui| {
                if ui.button("恢复整批").clicked() {
                    app.run_manage_restore_batch(batch.batch_id.clone());
                }
                if ui.button("永久清空").clicked() {
                    app.request_manage_purge_batch_confirmation(batch.batch_id.clone());
                }
            });
        });
        ui.add_space(4.0);
    }

    if ui.button("一键清空全部").clicked() {
        app.request_manage_purge_all_confirmation();
    }
}

fn render_action_bar(ui: &mut Ui, app: &mut MigratorApp) {
    let selected_count = app.manage_selected_ids.len();
    let visible_count = app.manage_filtered_rows().len();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("批量操作").strong());
        ui.separator();
        ui.label(RichText::new(format!("已选择 {} 条会话", selected_count)).strong());
        if selected_count == 0 {
            ui.label(
                RichText::new("先勾选会话后，才能导出、归档或删除到工具回收站。")
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
        ui.checkbox(
            &mut app.create_backup_on_manage_delete,
            "删除前创建安全备份",
        );

        if ui
            .add_enabled(visible_count > 0, egui::Button::new("全选当前结果"))
            .clicked()
        {
            app.select_all_filtered_manage_rows();
        }

        if ui
            .add_enabled(selected_count > 0, egui::Button::new("导出所选"))
            .clicked()
        {
            if let Some(path) = FileDialog::new()
                .add_filter("Codex 历史迁移包", &["codexhist"])
                .set_file_name("codex-selected-history.codexhist")
                .save_file()
            {
                app.run_manage_export(path);
            }
        }

        if ui
            .add_enabled(selected_count > 0, egui::Button::new("归档所选"))
            .clicked()
        {
            app.run_manage_archive_toggle(true);
        }

        if ui
            .add_enabled(selected_count > 0, egui::Button::new("取消归档"))
            .clicked()
        {
            app.run_manage_archive_toggle(false);
        }

        if ui
            .add_enabled(selected_count > 0, egui::Button::new("删除到工具回收站"))
            .clicked()
        {
            app.request_manage_delete_confirmation();
        }

        if ui
            .add_enabled(selected_count > 0, egui::Button::new("清空选择"))
            .clicked()
        {
            app.clear_manage_selection();
        }
    });
}

fn render_metadata_grid(
    ui: &mut Ui,
    app: &mut MigratorApp,
    row: &crate::models::manage::ManageRow,
) {
    egui::Grid::new("manage_detail_meta_grid")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .min_col_width(88.0)
        .show(ui, |ui| {
            render_meta_label(ui, "线程 ID");
            render_meta_value_with_copy(ui, app, "thread-id", "线程 ID", &row.id, &row.id, true);
            ui.end_row();

            render_meta_label(ui, "Provider");
            render_meta_value(ui, &row.model_provider, false);
            ui.end_row();

            render_meta_label(ui, "模型");
            render_meta_value(ui, row.model.as_deref().unwrap_or("未记录"), false);
            ui.end_row();

            render_meta_label(ui, "项目路径");
            render_meta_value_with_copy(
                ui,
                app,
                "project-path",
                "项目路径",
                &row.cwd_display,
                &row.cwd_display,
                true,
            );
            ui.end_row();

            render_meta_label(ui, "归档状态");
            render_meta_value(
                ui,
                if row.archived {
                    "已归档"
                } else {
                    "未归档"
                },
                false,
            );
            ui.end_row();

            render_meta_label(ui, "Payload 路径");
            render_meta_value_with_copy(
                ui,
                app,
                "payload-path",
                "Payload 路径",
                &row.rollout_path_display,
                &row.rollout_path_display,
                true,
            );
            ui.end_row();

            render_meta_label(ui, "健康状态");
            health_badge(ui, row.health);
            ui.end_row();
        });
}

fn render_meta_label(ui: &mut Ui, label: &str) {
    ui.label(
        RichText::new(label)
            .small()
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
}

fn render_meta_value(ui: &mut Ui, value: &str, monospace: bool) -> egui::Response {
    let display = if monospace {
        clip_text(value, 88)
    } else {
        clip_text(value, 120)
    };
    if monospace {
        ui.add(
            egui::Label::new(
                RichText::new(display)
                    .monospace()
                    .small()
                    .color(ui.visuals().weak_text_color()),
            )
            .selectable(true),
        )
    } else {
        ui.add(egui::Label::new(display).wrap().selectable(false))
    }
}

fn render_meta_value_with_copy(
    ui: &mut Ui,
    app: &mut MigratorApp,
    copy_key: &str,
    copy_label: &str,
    display_value: &str,
    copy_value: &str,
    monospace: bool,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let text_hover = format!("{copy_value}\n单击复制");
        let text_response =
            render_meta_value(ui, display_value, monospace).on_hover_text(text_hover);

        let copied = app.manage_copy_feedback_active(copy_key);
        let button_text = if copied { "✓" } else { "⧉" };
        let button_hover = if copied {
            format!("已复制{copy_label}")
        } else {
            format!("复制{copy_label}")
        };
        let button_response = ui
            .add_enabled(
                !copy_value.trim().is_empty(),
                egui::Button::new(RichText::new(button_text).small()),
            )
            .on_hover_text(button_hover);

        if text_response.clicked() || button_response.clicked() {
            ui.ctx().copy_text(copy_value.to_string());
            app.record_manage_copy(copy_key);
        }
    });
}

fn soft_badge(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(format!(" {text} "))
            .small()
            .background_color(ui.visuals().faint_bg_color)
            .color(ui.visuals().weak_text_color()),
    );
}

fn clip_text(text: &str, max_chars: usize) -> String {
    let mut clipped = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            clipped.push('…');
            return clipped;
        }
        clipped.push(ch);
    }
    clipped
}

fn render_confirmation_dialogs(ctx: &egui::Context, app: &mut MigratorApp) {
    render_delete_confirmation(ctx, app);
    render_purge_batch_confirmation(ctx, app);
    render_purge_all_confirmation(ctx, app);
}

fn render_delete_confirmation(ctx: &egui::Context, app: &mut MigratorApp) {
    let mut open = app.manage_confirm_delete_open;
    let mut confirm = false;
    let mut cancel = false;

    if open {
        egui::Window::new("确认删除到回收站")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!(
                    "即将把 {} 条会话移入工具回收站。",
                    app.manage_selected_ids.len()
                ));
                ui.label("这不会直接永久删除，你仍可在回收站里恢复整批数据。");
                if app.create_backup_on_manage_delete {
                    ui.label("当前已开启“删除前创建安全备份”。");
                } else {
                    ui.label("当前已关闭“删除前创建安全备份”。");
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("确认删除").clicked() {
                        confirm = true;
                    }
                    if ui.button("取消").clicked() {
                        cancel = true;
                    }
                });
            });
    }

    if confirm {
        app.confirm_manage_delete();
    } else if cancel || (app.manage_confirm_delete_open && !open) {
        app.cancel_manage_delete_confirmation();
    }
}

fn render_purge_batch_confirmation(ctx: &egui::Context, app: &mut MigratorApp) {
    let Some(batch_id) = app.manage_confirm_purge_batch_id.clone() else {
        return;
    };

    let mut open = true;
    let mut confirm = false;
    let mut cancel = false;
    let summary = app
        .manage_trash_batches
        .iter()
        .find(|batch| batch.batch_id == batch_id)
        .cloned();

    egui::Window::new("确认永久清空批次")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label("该操作只会清理工具回收站中的这一批内容，不能撤销。");
            ui.label(format!("批次 ID：{batch_id}"));
            if let Some(summary) = &summary {
                ui.label(format!(
                    "包含 {} 条会话 / {} 个 payload。",
                    summary.thread_count, summary.payload_count
                ));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("确认清空").clicked() {
                    confirm = true;
                }
                if ui.button("取消").clicked() {
                    cancel = true;
                }
            });
        });

    if confirm {
        app.confirm_manage_purge_batch();
    } else if cancel || !open {
        app.cancel_manage_purge_batch_confirmation();
    }
}

fn render_purge_all_confirmation(ctx: &egui::Context, app: &mut MigratorApp) {
    let mut open = app.manage_confirm_purge_all_open;
    let mut confirm = false;
    let mut cancel = false;

    if open {
        let batch_count = app.manage_trash_batches.len();
        let thread_count = app
            .manage_trash_batches
            .iter()
            .map(|batch| batch.thread_count)
            .sum::<usize>();
        egui::Window::new("确认清空全部回收站")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("该操作会永久删除工具回收站中的全部批次，不能撤销。");
                ui.label(format!(
                    "当前将清理 {} 个批次，共 {} 条会话。",
                    batch_count, thread_count
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("确认清空全部").clicked() {
                        confirm = true;
                    }
                    if ui.button("取消").clicked() {
                        cancel = true;
                    }
                });
            });
    }

    if confirm {
        app.confirm_manage_purge_all();
    } else if cancel || (app.manage_confirm_purge_all_open && !open) {
        app.cancel_manage_purge_all_confirmation();
    }
}

fn archived_filter_label(filter: ArchivedFilter) -> &'static str {
    match filter {
        ArchivedFilter::All => "全部",
        ArchivedFilter::ActiveOnly => "仅未归档",
        ArchivedFilter::ArchivedOnly => "仅已归档",
    }
}

fn health_filter_label(filter: HealthFilter) -> &'static str {
    match filter {
        HealthFilter::All => "全部",
        HealthFilter::HealthyOnly => "正常",
        HealthFilter::NeedsAttentionOnly => "需处理",
        HealthFilter::MissingPayloadOnly => "缺失 Payload",
        HealthFilter::InvalidPathOnly => "异常路径",
        HealthFilter::ArchiveStateMismatchOnly => "归档目录不一致",
    }
}

fn health_badge(ui: &mut Ui, health: ManageHealth) {
    let (label, color) = match health {
        ManageHealth::Healthy => ("正常", Color32::from_rgb(36, 122, 76)),
        ManageHealth::MissingPayload => ("缺失 Payload", Color32::from_rgb(164, 89, 23)),
        ManageHealth::InvalidPath => ("异常路径", Color32::from_rgb(171, 51, 51)),
        ManageHealth::ArchiveStateMismatch => ("归档目录不一致", Color32::from_rgb(124, 58, 237)),
    };
    ui.label(
        RichText::new(format!(" {label} "))
            .strong()
            .small()
            .color(color)
            .background_color(color.linear_multiply(0.10)),
    );
}

fn open_path(path: &Path) -> Result<(), String> {
    let command = open_path_command(path)?;
    run_platform_command(&command)
}

fn open_file_location(path: &Path) -> Result<(), String> {
    let command = open_file_location_command(path)?;
    run_platform_command(&command)
}
