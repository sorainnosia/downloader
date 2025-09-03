// Advanced Image Resizer with Beautiful UI
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use iced::{
    executor, font, theme,
    widget::{button, column, container, progress_bar, row, scrollable, text, text_input, Space,
	horizontal_rule},
    Alignment, Application, Color, Command, Element, Font, Length, Settings, Size, Theme, Background, Border,
};
use futures_util::StreamExt;
use atomic_float::AtomicF64;
use iced::{Subscription, subscription};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tokio::io::{AsyncSeekExt}; // brings .seek() into scopeuse url::Url;
use url::Url;
use futures::future::join_all;
use reqwest::blocking::Client;
use std::fs::OpenOptions;
use std::io::{Read, Write, Seek, SeekFrom}; // <-- bring blocking traits into scope

const NOTO_SANS_REGULAR: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");
const NOTO_SANS_BOLD: &[u8] = include_bytes!("../assets/NotoSans-Bold.ttf");

// Download Url Example
// github assets link :
//   https://github.com/sorainnosia/image-resizer-advanced/releases/expanded_assets/0.1.1
// huggingface api link : 
//   https://huggingface.co/api/datasets/facebook/flores
// sourceforge.net files link :
//   https://sourceforge.net/projects/czkawka.mirror/files/10.0.0/
// archive.org project link :
//   https://archive.org/download/ms_solitaire_windows_xp
// wikimedia.org project link :
//   https://commons.wikimedia.org/wiki/Category:Scenic_wallpaper

// Font definitions
const HEADING_FONT: Font = Font {
    family: font::Family::Name("Noto Sans"),
    weight: font::Weight::Bold,
    stretch: font::Stretch::Normal,
    style: font::Style::Normal,
};

const BODY_FONT: Font = Font {
    family: font::Family::Name("Noto Sans"),
    weight: font::Weight::Normal,
    stretch: font::Stretch::Normal,
    style: font::Style::Normal,
};

const LIGHT_FONT: Font = Font {
    family: font::Family::Name("Noto Sans"),
    weight: font::Weight::Light,
    stretch: font::Stretch::Normal,
    style: font::Style::Normal,
};

// Theme colors
const PRIMARY_COLOR: Color = Color::from_rgb(0.2, 0.5, 0.9);
const SECONDARY_COLOR: Color = Color::from_rgb(0.9, 0.95, 1.0);
const SUCCESS_COLOR: Color = Color::from_rgb(0.2, 0.7, 0.3);
const ERROR_COLOR: Color = Color::from_rgb(0.9, 0.2, 0.2);
const WARNING_COLOR: Color = Color::from_rgb(0.9, 0.6, 0.0);
const BACKGROUND_COLOR: Color = Color::from_rgb(0.97, 0.97, 0.98);
const CARD_COLOR: Color = Color::WHITE;
const TEXT_COLOR: Color = Color::from_rgb(0.2, 0.2, 0.3);
const TEXT_SECONDARY: Color = Color::from_rgb(0.4, 0.4, 0.5);
const TEXT_MUTED: Color = Color::from_rgb(0.6, 0.6, 0.7);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DomainConfig {
    #[serde(rename = "Filter")]
    filter: Vec<Vec<String>>,
    #[serde(rename = "Ignore")]
    ignore: Vec<String>,
    #[serde(rename = "DownloadUrl", skip_serializing_if = "Option::is_none")]
    download_url: Option<String>,
    #[serde(rename = "UrlItems", skip_serializing_if = "Option::is_none")]
    url_items: Option<HashMap<String, Vec<String>>>,
    #[serde(rename = "UrlStartItems", skip_serializing_if = "Option::is_none")]
    url_start_items: Option<HashMap<String, String>>,
	#[serde(rename = "UrlReplaceItems", skip_serializing_if = "Option::is_none")]
    url_replace_items: Option<HashMap<String, Vec<String>>>,
}

#[derive(Debug, Clone)]
struct DownloadItem {
    url: String,
    filename: String,
	name_betw: String,
    progress: f64,
    status: DownloadStatus,
    total_size: f64,
    downloaded: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum DownloadStatus {
    Pending,
    Downloading,
    Completed,
	Cancelling,
    Failed(String),
    Skipped, // New status for files that already exist
}

#[derive(Debug, Clone)]
enum Message {
	WindowCloseRequested,
	ForceExit,
    UrlChanged(String),
    OutputFolderChanged(String),
    ParallelChanged(String),
    ChunkSizeChanged(String),
    StartDownload,
    CancelDownload,
	UpdateCancelledDownloads,
    ProcessNextUrl(String),
    AddDownloadItems(Vec<DownloadItem>),
    StartDownloadItem(usize),
    UpdateDownloadProgress(usize, u64, u64),
    UpdateDownloadStatus(usize, DownloadStatus),
    UpdateTotalSize(usize, u64),
    ProcessNextBatch,
    Tick,
}

struct Downloader {
    url_input: String,
    output_folder: String,
    parallel_input: String,
    chunk_size_input: String,
    is_downloading: bool,
    download_items: Vec<DownloadItem>,
    config: HashMap<String, DomainConfig>,
    cancel_flag: Arc<AtomicBool>,
    urls_to_process: Vec<String>,
    active_downloads: Arc<Mutex<HashMap<usize, Arc<AtomicF64>>>>,
	cancel: Arc<tokio_util::sync::CancellationToken>, // add tokio-util = "0.7"
}

// Just add this to your existing Downloader implementation:

fn main() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = Size::new(900.0, 450.0);
    settings.window.min_size = Some(Size::new(800.0, 450.0));
    settings.fonts = vec![
        include_bytes!("../assets/NotoSans-Regular.ttf").as_slice().into(),
        include_bytes!("../assets/NotoSans-Bold.ttf").as_slice().into(),
    ];
    settings.default_font = BODY_FONT;
    settings.default_text_size = 14.into();
    
    Downloader::run(settings)
}

impl Application for Downloader {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
	type Flags = ();
	
	// APPROACH 3: Simpler subscription approach
	fn subscription(&self) -> Subscription<Message> {
		use iced::event::{self, Event};
		use iced::window;
		
		let window_events = event::listen_with(|event, _status| {
			// Debug print to see what events are coming through
			if let Event::Window(_, _) = &event {
				println!("Window event received: {:?}", event);
			}
			
			let mut closed = false;
			let com = match event {
				Event::Window(id, event) => {
					match event {
						window::Event::CloseRequested => {
							//let ct = self.cancel.clone();
							//tokio::spawn(move || ct.cancel());
							closed = true;
							println!("Close requested!");
							Some(Message::WindowCloseRequested)
						}
						_ => None
					}
				}
				_ => None,
			};
			if closed {
				//let ct = self.cancel.clone();
				//ct.cancel();
			}
			com
		});
		
		if self.is_downloading {
			Subscription::batch(vec![
				window_events,
				iced::time::every(Duration::from_millis(100))
					.map(|_| Message::Tick),
			])
		} else {
			window_events
		}
	}
	
    fn new(_flags: ()) -> (Self, Command<Message>) {
        let config = load_config();
        
        (
            Self {
                url_input: String::new(),
                output_folder: std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                parallel_input: "7".to_string(),
                chunk_size_input: "100MB".to_string(),
                is_downloading: false,
                download_items: Vec::new(),
                config,
                cancel_flag: Arc::new(AtomicBool::new(false)),
                urls_to_process: Vec::new(),
                active_downloads: Arc::new(Mutex::new(HashMap::new())),
				cancel: Arc::new(tokio_util::sync::CancellationToken::new())
            },
            Command::none()
        )
    }

    fn title(&self) -> String {
        String::from("Downloader Pro")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::UrlChanged(url) => {
                self.url_input = url;
                Command::none()
            }
			
			Message::ForceExit => {
				std::process::exit(0);
				Command::none()
			}
			Message::WindowCloseRequested => {
				self.cancel.cancel();
				println!("Close Requested");
				if self.is_downloading {
					println!("Close Requested during download");
					eprintln!("Window closing - cancelling downloads...");
					
					// Cancel all downloads
					self.cancel_flag.store(true, Ordering::Relaxed);
					
					// Mark all downloading items as cancelled
					for item in &mut self.download_items {
						if matches!(item.status, DownloadStatus::Downloading) {
							item.status = DownloadStatus::Failed("Cancelled".to_string());
						}
					}
					
					self.is_downloading = false;
					
					// Give tasks 500ms to clean up
					Command::perform(
						async {
							tokio::time::sleep(Duration::from_millis(500)).await;
						},
						|_| Message::ForceExit
					)
				} else {
					std::process::exit(0);
					Command::none()
				}
			}

            Message::OutputFolderChanged(folder) => {
                self.output_folder = folder;
                Command::none()
            }
            Message::ParallelChanged(parallel) => {
                self.parallel_input = parallel;
                Command::none()
            }
            Message::ChunkSizeChanged(chunk_size) => {
                self.chunk_size_input = chunk_size;
                Command::none()
            }
            Message::StartDownload => {
                if !self.url_input.is_empty() {
                    self.is_downloading = true;
					self.cancel_flag = Arc::new(AtomicBool::new(false));
					self.cancel = Arc::new(tokio_util::sync::CancellationToken::new());

                    self.download_items.clear();
                    self.cancel_flag.store(false, Ordering::Relaxed);
                    let _ = self.active_downloads.try_lock().map(|mut m| m.clear());
                    
                    // Generate URLs from input
                    self.urls_to_process = generate_urls(&self.url_input);
                    
                    if let Some(first_url) = self.urls_to_process.first().cloned() {
                        return Command::batch(vec![
                            Command::perform(async move { first_url }, Message::ProcessNextUrl),
                            Command::perform(async {}, |_| Message::Tick),
                        ]);
                    }
                }
                Command::none()
            }
            Message::CancelDownload => {
                self.is_downloading = false;
                self.cancel_flag.store(true, Ordering::Relaxed);
				self.cancel.cancel();
                Command::perform(
					async {
						tokio::time::sleep(Duration::from_millis(500)).await;
					},
					|_| Message::UpdateCancelledDownloads
				)
            }
			Message::UpdateCancelledDownloads => {
				for item in &mut self.download_items {
					if matches!(item.status, DownloadStatus::Cancelling) {
						item.status = DownloadStatus::Failed("Cancelled by user".to_string());
					}
				}
				Command::none()
			}
            Message::ProcessNextUrl(url) => {
                let config = self.config.clone();
                
                Command::perform(
                    process_url(url.clone(), config),
                    move |result| match result {
                        Ok(items) => Message::AddDownloadItems(items),
                        Err(e) => {
                            eprintln!("Error processing URL {}: {}", url, e);
                            Message::ProcessNextBatch
                        }
                    }
                )
            }
            Message::AddDownloadItems(items) => {
                let start_index = self.download_items.len();
                
                // Add items and initialize their states
                for item in items {
                    self.download_items.push(item);
                }
                
                // Start downloading first item
                if start_index < self.download_items.len() {
                    Command::batch(vec![
                        Command::perform(async move { start_index }, Message::StartDownloadItem),
                        Command::perform(async {}, |_| Message::Tick),
                    ])
                } else {
                    Command::none()
                }
            }
            Message::StartDownloadItem(index) => {
                if let Some(item) = self.download_items.get_mut(index) {
                    if matches!(item.status, DownloadStatus::Pending) {
                        item.status = DownloadStatus::Downloading;
                        
						let name_betw = item.name_betw.to_string();
                        let url = item.url.clone();
                        let filename = item.filename.clone();
                        let output_folder = self.output_folder.clone();
                        let parallel = self.parallel_input.parse::<usize>().unwrap_or(1);
                        let chunk_size = parse_chunk_size(&self.chunk_size_input);
                        let cancel_flag = self.cancel_flag.clone();
                        let active_downloads = self.active_downloads.clone();
                        let cancel_token = self.cancel.clone();
                        // First get the file size
                        let url_clone = url.clone();
                        return Command::batch(vec![
                            Command::perform(
                                async move {
                                    let client = reqwest::Client::builder()
									//.timeout(Duration::from_secs(30))
									//.connect_timeout(Duration::from_secs(5))
									.build()
									.map_err(|e| format!("Client build error: {e}")).ok()?;
									
                                    let head_response = client.head(&url_clone)
                                        .send()
                                        .await
                                        .ok()?;
                                    
                                    head_response
                                        .headers()
                                        .get(reqwest::header::CONTENT_LENGTH)
                                        .and_then(|v| v.to_str().ok())
                                        .and_then(|v| v.parse::<u64>().ok())
                                },
                                move |size| {
                                    if let Some(total_size) = size {
                                        Message::UpdateTotalSize(index, total_size)
                                    } else {
                                        Message::UpdateTotalSize(index, 0)
                                    }
                                }
                            ),
                            Command::perform(
                                download_file_async(
                                    index,
                                    url,
                                    filename,
									name_betw,
                                    output_folder,
                                    parallel,
                                    chunk_size,
                                    cancel_flag,
                                    active_downloads,
									cancel_token.clone()
                                ),
                                move |result| match result {
                                    Ok((total, downloaded)) => {
                                        if total == downloaded && downloaded == u64::MAX {
                                            // Special case for skipped files
                                            Message::UpdateDownloadStatus(index, DownloadStatus::Skipped)
                                        } else {
                                            Message::UpdateDownloadStatus(index, DownloadStatus::Completed)
                                        }
                                    },
                                    Err(e) => Message::UpdateDownloadStatus(index, DownloadStatus::Failed(e)),
                                }
                            ),
                        ]);
                    }
                }
                
                // Check for next item to download
                if index + 1 < self.download_items.len() {
                    Command::perform(async move { index + 1 }, Message::StartDownloadItem)
                } else {
                    Command::perform(async {}, |_| Message::ProcessNextBatch)
                }
            }
            Message::UpdateTotalSize(index, total_size) => {
                if let Some(item) = self.download_items.get_mut(index) {
                    item.total_size = total_size as f64;
                }
                Command::none()
            }
            Message::UpdateDownloadProgress(index, downloaded, total) => {
                if let Some(item) = self.download_items.get_mut(index) {
                    item.downloaded = downloaded as f64;
                    item.total_size = total as f64;
                    item.progress = if total > 0 {
                        downloaded as f64 / total as f64
                    } else {
                        0.0
                    };
                }
                Command::none()
            }
            Message::UpdateDownloadStatus(index, status) => {
                if let Some(item) = self.download_items.get_mut(index) {
                    item.status = status.clone();
                    
                    if matches!(item.status, DownloadStatus::Completed | DownloadStatus::Skipped) {
                        item.progress = 1.0;
                        item.downloaded = item.total_size;
                    }
                    
                    // Clean up active downloads when complete
                    if matches!(status, DownloadStatus::Completed | DownloadStatus::Failed(_) | DownloadStatus::Skipped) {
                        let active_downloads = self.active_downloads.clone();
                        tokio::spawn(async move {
                            let mut downloads = active_downloads.lock().await;
                            downloads.remove(&index);
                        });
                    }
                }
                
                // Check for next item to download after this one completes
                if matches!(status, DownloadStatus::Completed | DownloadStatus::Failed(_) | DownloadStatus::Skipped) {
                    if index + 1 < self.download_items.len() {
                        return Command::perform(async move { index + 1 }, Message::StartDownloadItem);
                    } else {
                        return Command::perform(async {}, |_| Message::ProcessNextBatch);
                    }
                }
                
                Command::none()
            }
            Message::ProcessNextBatch => {
                if !self.urls_to_process.is_empty() {
                    self.urls_to_process.remove(0);
                    
                    if let Some(next_url) = self.urls_to_process.first().cloned() {
                        return Command::perform(async move { next_url }, Message::ProcessNextUrl);
                    }
                }
                
                // Check if all downloads are complete
                let all_complete = self.download_items.iter().all(|item| {
                    matches!(item.status, DownloadStatus::Completed | DownloadStatus::Failed(_) | DownloadStatus::Skipped)
                });
                
                if all_complete {
                    self.is_downloading = false;
                }
                
                Command::none()
            }
            Message::Tick => {
				// Update progress from active downloads
				if let Ok(downloads) = self.active_downloads.try_lock() {
					for (index, progress) in downloads.iter() {
						let downloaded = progress.load(Ordering::Relaxed);
						if let Some(item) = self.download_items.get_mut(*index) {
							// This is correct - downloaded is in bytes
							if item.downloaded != downloaded && item.total_size > 0.0 {
								item.downloaded = downloaded;
								item.progress = downloaded / item.total_size;
							}
						}
					}
				}
				
				// Reschedule tick if still downloading
				if self.is_downloading {
					Command::perform(
						async {
							tokio::time::sleep(Duration::from_millis(100)).await;
						},
						|_| Message::Tick
					)
				} else {
					Command::none()
				}
			}
        }
    }

    fn view(&self) -> Element<Message> {
        // Header section with gradient background
        let header = container(
            column![
                text("Downloader Pro")
                    .size(16)
                    .font(HEADING_FONT)
                    .style(Color::WHITE),
                text("Fast and efficient file downloads")
                    .size(11)
                    .font(LIGHT_FONT)
                    .style(Color::from_rgba(1.0, 1.0, 1.0, 0.8)),
            ].spacing(4)
        )
        .width(Length::Fill)
        .padding([18, 26])
        .style(theme::Container::Custom(Box::new(GradientContainer)));

        // URL Input Card
        let url_card = card_container(
            column![
                section_title("Download URL"),
				
				 // Output folder
                labeled_input(
                    "Use {placeholder} for sequences (e.g., file_{placeholder}.jpg for 1-9999)",
                    text_input("Enter URL...", &self.url_input)
						.on_input(Message::UrlChanged)
						.padding([4, 8])
						.font(BODY_FONT)
						.size(14),
                ),
                
                
            ].spacing(8)
        );

        // Settings Card
        let settings_card = card_container(
            column![
                section_title("Download Settings"),
                
                // Output folder
                labeled_input(
                    "Output Folder",
                    text_input("Select output folder...", &self.output_folder)
                        .on_input(Message::OutputFolderChanged)
                        .padding([4, 8])
                        .font(BODY_FONT)
                        .size(14)
                ),
                
                Space::with_height(16),
                
                // Parallel downloads and chunk size
                row![
                    labeled_input(
                        "Parallel Downloads",
                        text_input("1", &self.parallel_input)
                            .on_input(Message::ParallelChanged)
                            .padding([4, 8])
                            .width(Length::Fixed(100.0))
                            .font(BODY_FONT)
                            .size(14)
                    ),
                    Space::with_width(24),
                    labeled_input(
                        "Chunk Size",
                        text_input("100MB", &self.chunk_size_input)
                            .on_input(Message::ChunkSizeChanged)
                            .padding([4, 8])
                            .width(Length::Fixed(120.0))
                            .font(BODY_FONT)
                            .size(14)
                    ),
                ].spacing(16),
            ].spacing(0)
        );

        // Action buttons
        let action_section = container(
            if self.is_downloading {
                button(
                    text("Cancel Download")
                        .font(HEADING_FONT)
                        .size(14)
                        .horizontal_alignment(iced::alignment::Horizontal::Center)
                )
                .on_press(Message::CancelDownload)
                .padding([8, 10])
                .style(theme::Button::Destructive)
                .width(Length::Fixed(180.0))
            } else {
                button(
                    text("Start Download")
                        .font(HEADING_FONT)
                        .size(13)
                        .horizontal_alignment(iced::alignment::Horizontal::Center)
                )
                .on_press(Message::StartDownload)
                .padding([8, 10])
                .style(theme::Button::Primary)
                .width(Length::Fixed(180.0))
            }
        )
        .width(Length::Fill)
        .center_x();

        // Downloads table
        let downloads_card = if !self.download_items.is_empty() {
            let table_header = container(
                row![
                    container(text("Filename").font(HEADING_FONT).size(12))
                        .width(Length::FillPortion(4))
                        .padding([10, 16]),
                    container(text("Size").font(HEADING_FONT).size(12))
                        .width(Length::FillPortion(2))
                        .center_x()
                        .padding([10, 16]),
                    container(text("Progress").font(HEADING_FONT).size(12))
                        .width(Length::FillPortion(3))
                        .center_x()
                        .padding([10, 16]),
                    container(text("Status").font(HEADING_FONT).size(12))
                        .width(Length::FillPortion(2))
                        .center_x()
                        .padding([10, 16]),
                ]
            )
            .style(theme::Container::Custom(Box::new(TableHeaderContainer)))
            .width(Length::Fill);

            let mut table_body = column![].spacing(0);
            
            for (i, item) in self.download_items.iter().enumerate() {
                let status_color = match &item.status {
					DownloadStatus::Pending => TEXT_MUTED,
					DownloadStatus::Downloading => PRIMARY_COLOR,
					DownloadStatus::Cancelling => WARNING_COLOR, // Orange/yellow for cancelling
					DownloadStatus::Completed => SUCCESS_COLOR,
					DownloadStatus::Failed(_) => ERROR_COLOR,
					DownloadStatus::Skipped => WARNING_COLOR,
				};

				let status_text = match &item.status {
					DownloadStatus::Pending => "Pending",
					DownloadStatus::Downloading => "Downloading",
					DownloadStatus::Cancelling => "Cancelling...",
					DownloadStatus::Completed => "Completed",
					DownloadStatus::Failed(err) => {
						if err.len() > 20 {
							&err[..20]
						} else {
							err
						}
					},
					DownloadStatus::Skipped => "Already Exists",
				};
                
                let size_text = format_size(item.downloaded, item.total_size);
                
                let display_filename = if item.filename.contains('%') {
                    urlencoding::decode(&item.filename)
                        .unwrap_or(std::borrow::Cow::Borrowed(&item.filename))
                        .to_string()
                } else {
                    item.filename.clone()
                };
                
                let truncated_filename = if display_filename.len() > 50 {
                    format!("{}...{}", 
                        &display_filename[..35], 
                        &display_filename[display_filename.len()-12..]
                    )
                } else {
                    display_filename
                };
                
                let row_content = row![
                    container(
                        text(truncated_filename)
                            .font(BODY_FONT)
                            .size(12)
                            .style(TEXT_COLOR)
                    )
                    .width(Length::FillPortion(4))
                    .padding([12, 16]),
                    
                    container(
                        text(size_text)
                            .font(BODY_FONT)
                            .size(13)
                            .style(TEXT_SECONDARY)
                    )
                    .width(Length::FillPortion(2))
                    .center_x()
                    .padding([12, 16]),
                    
                    container(
                        column![
                            progress_bar(0.0..=1.0, item.progress as f32)
                                .height(Length::Fixed(6.0))
                                .style(theme::ProgressBar::Primary),
                            text(format!("{:.0}%", item.progress * 100.0))
                                .font(BODY_FONT)
                                .size(11)
                                .style(TEXT_SECONDARY),
                        ].spacing(4)
                    )
                    .width(Length::FillPortion(3))
                    .padding([8, 16]),
                    
                    container(
                        text(status_text)
                            .font(BODY_FONT)
                            .size(12)
                            .style(status_color)
                    )
                    .width(Length::FillPortion(2))
                    .center_x()
                    .padding([12, 16]),
                ];
                
                let row_container = if i % 2 == 0 {
                    container(row_content)
                        .width(Length::Fill)
                        .style(theme::Container::Custom(Box::new(TableRowEven)))
                } else {
                    container(row_content)
                        .width(Length::Fill)
                        .style(theme::Container::Custom(Box::new(TableRowOdd)))
                };
                
                table_body = table_body.push(row_container);
            }
            
            card_container(
                column![
                    row![
                        section_title("Downloads"),
                        Space::with_width(Length::Fill),
                        text(format!("{} file(s)", self.download_items.len()))
                            .font(BODY_FONT)
                            .size(12)
                            .style(TEXT_MUTED),
                    ],
                    Space::with_height(16),
                    table_header,
                    container(
						table_body
                        //scrollable(table_body)
                        //    .height(Length::Fixed(300.0))
                    )
                    .style(theme::Container::Custom(Box::new(TableContainer)))
                    .width(Length::Fill),
                ].spacing(0)
            )
        } else {
            container(column![]).into()
        };

        // Main layout
        let content = scrollable(
            column![
                header,
                container(
                    column![
                        url_card,
                        settings_card,
                        action_section,
                        downloads_card,
                        Space::with_height(20),
                    ].spacing(16)
                )
                //.max_width(1200)
				.width(Length::Fill)
                .center_x()
                .padding([6, 14, 6, 6])
            ].spacing(0)
        );

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::Container::Custom(Box::new(BackgroundContainer)))
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }
}

// Helper UI components
fn section_title(title: &str) -> Element<'static, Message> {
    text(title)
        .size(14)
        .font(HEADING_FONT)
        .style(TEXT_COLOR)
        .into()
}

fn labeled_input<'a>(label: &str, input: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        text(label)
            .size(13)
            .font(BODY_FONT)
            .style(TEXT_SECONDARY),
        Space::with_height(6),
        input.into(),
    ].spacing(0).into()
}

fn card_container<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .padding(14)
        .style(theme::Container::Custom(Box::new(CardContainer)))
        .into()
}

fn format_size(downloaded: f64, total: f64) -> String {
    if total > 0.0 {
        let downloaded_mb = downloaded as f64 / 1_048_576.0;
        let total_mb = total as f64 / 1_048_576.0;
        if total_mb >= 1000.0 {
            format!("{:.1}GB / {:.1}GB", downloaded_mb / 1024.0, total_mb / 1024.0)
        } else {
            format!("{:.1}MB / {:.1}MB", downloaded_mb, total_mb)
        }
    } else if downloaded > 0.0 {
        let downloaded_mb = downloaded as f64 / 1_048_576.0;
        if downloaded_mb >= 1000.0 {
            format!("{:.1}GB", downloaded_mb / 1024.0)
        } else {
            format!("{:.1}MB", downloaded_mb)
        }
    } else {
        "—".to_string()
    }
}

// Container styles
struct BackgroundContainer;
impl container::StyleSheet for BackgroundContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(BACKGROUND_COLOR)),
            ..Default::default()
        }
    }
}

struct CardContainer;
impl container::StyleSheet for CardContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(CARD_COLOR)),
            border: iced::Border {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        }
    }
}

struct GradientContainer;
impl container::StyleSheet for GradientContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(PRIMARY_COLOR)),
            ..Default::default()
        }
    }
}

struct TableContainer;
impl container::StyleSheet for TableContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(Color::from_rgb(0.98, 0.98, 0.99))),
            border: iced::Border {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.1),
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

struct TableHeaderContainer;
impl container::StyleSheet for TableHeaderContainer {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(Color::from_rgb(0.93, 0.94, 0.96))),
            border: iced::Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: [8.0, 8.0, 0.0, 0.0].into(),
            },
            ..Default::default()
        }
    }
}

struct TableRowEven;
impl container::StyleSheet for TableRowEven {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(Color::WHITE)),
            ..Default::default()
        }
    }
}

struct TableRowOdd;
impl container::StyleSheet for TableRowOdd {
    type Style = Theme;
    
    fn appearance(&self, _style: &Self::Style) -> container::Appearance {
        container::Appearance {
            background: Some(Background::Color(Color::from_rgb(0.98, 0.98, 0.99))),
            ..Default::default()
        }
    }
}

// All the existing functions remain the same
fn load_config() -> HashMap<String, DomainConfig> {
    let exe_path = std::env::current_exe().unwrap_or_default();
    let exe_name = exe_path.file_stem().unwrap_or_default().to_string_lossy();
    let config_path = exe_path.parent().unwrap_or(&PathBuf::new()).join(format!("{}.json", exe_name));
    
    // If config file exists, load and return it
    if config_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str(&contents) {
                return config;
            }
        }
    }
    
    // Create default config only if file doesn't exist
    let mut default_config = HashMap::new();
    
    let mut url_items = HashMap::new();
    url_items.insert("type".to_string(), vec!["/api/".to_string(), "/".to_string()]);
    let mut url_start_items = HashMap::new();
    url_start_items.insert("repo".to_string(), "/api/".to_string());
	let mut url_replace_items = HashMap::new();
    url_replace_items.insert("models".to_string(), vec!["/models/".to_string(), "/".to_string()]);
    
    default_config.insert(
        "huggingface.co".to_string(),
        DomainConfig {
            filter: vec![
                vec!["\"rfilename\":\"".to_string(), "\"".to_string()],
            ],
            ignore: vec!["thumb".to_string(), "preview".to_string(), "icon".to_string(), "_small".to_string(), "_medium".to_string()],
            download_url: Some("https://huggingface.co/{repo}/resolve/main/{placeholder}".to_string()),
            url_items: Some(url_items),
			url_start_items: Some(url_start_items),
			url_replace_items: Some(url_replace_items)
        },
    );
	
    let mut url_items = HashMap::new();
    let mut url_start_items = HashMap::new();
	let mut url_replace_items = HashMap::new();
    
	default_config.insert(
        "github.com".to_string(),
        DomainConfig {
            filter: vec![
                vec!["<a href=\"".to_string(), "\"".to_string()],
            ],
            ignore: vec![],
            download_url: Some("https://github.com/{placeholder}".to_string()),
            url_items: Some(url_items),
			url_start_items: Some(url_start_items),
			url_replace_items: Some(url_replace_items)
        },
    );
	
	let mut url_items = HashMap::new();
    let mut url_start_items = HashMap::new();
	let mut url_replace_items = HashMap::new();
    
	default_config.insert(
        "sourceforge.net".to_string(),
        DomainConfig {
            filter: vec![
                vec!["<tr title=\"".to_string(), "</tr>".to_string()],
				vec!["<a href=\"https://sourceforge.net/projects/".to_string(), "/download\"".to_string()],
            ],
            ignore: vec![],
            download_url: Some("https://sourceforge.net/projects/{placeholder}/download".to_string()),
            url_items: Some(url_items),
			url_start_items: Some(url_start_items),
			url_replace_items: Some(url_replace_items)
        },
    );
	
	let mut url_items = HashMap::new();
    let mut url_start_items = HashMap::new();
    url_start_items.insert("repo".to_string(), "/download/".to_string());
	let mut url_replace_items = HashMap::new();
    
	default_config.insert(
        "archive.org".to_string(),
        DomainConfig {
            filter: vec![
                vec!["<td><a href=\"".to_string(), "\"".to_string()],
            ],
            ignore: vec!["/details/".to_string()],
            download_url: Some("https://archive.org/download/{repo}/{placeholder}".to_string()),
            url_items: Some(url_items),
			url_start_items: Some(url_start_items),
			url_replace_items: Some(url_replace_items)
        },
    );
    
    // Only write default config if file doesn't exist
    if !config_path.exists() {
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = std::fs::write(&config_path, json);
        }
    }
    
    default_config
}

fn generate_urls(input: &str) -> Vec<String> {
    if input.contains("{placeholder}") {
        (1..=9999)
            .map(|i| input.replace("{placeholder}", &i.to_string()))
            .collect()
    } else {
        vec![input.to_string()]
    }
}

fn parse_chunk_size(input: &str) -> usize {
    let input = input.trim().to_uppercase();
    let (number_part, unit_part) = input.split_at(
        input.find(|c: char| c.is_alphabetic()).unwrap_or(input.len())
    );
    
    let number: f64 = number_part.parse().unwrap_or(100.0);
    
    let multiplier = match unit_part {
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        _ => 1024 * 1024, // Default to MB
    };
    
    (number * multiplier as f64) as usize
}

async fn process_url(
    url: String,
    config: HashMap<String, DomainConfig>,
) -> Result<Vec<DownloadItem>, String> {
    // Extract domain from URL
    let parsed_url = Url::parse(&url).map_err(|e| e.to_string())?;
    let domain = parsed_url.host_str().ok_or("Invalid URL")?;
    
    // Check if domain exists in config
    if let Some(domain_config) = config.get(domain) {
        // Extract URL items from the current URL if UrlItems or UrlStartItems are configured
        let mut url_replacements: HashMap<String, String> = HashMap::new();
        
        // Process UrlItems (extract between start and end patterns)
        if let Some(url_items) = &domain_config.url_items {
            for (key, patterns) in url_items {
                if patterns.len() == 2 {
                    let start = &patterns[0];
                    let end = &patterns[1];
                    
                    // Extract value from URL using the patterns
                    if let Some(extracted_values) = extract_between(&format!("{}/", &url), start, end).first() {
                        if !extracted_values.is_empty() {
                            url_replacements.insert(format!("{{{}}}", key), extracted_values.clone());
                        }
                    }
                }
            }
        }
        
        // Process UrlStartItems (extract everything after the starting point)
        if let Some(url_start_items) = &domain_config.url_start_items {
            for (key, start_pattern) in url_start_items {
                // Find the starting point in the URL
                if let Some(start_pos) = url.find(start_pattern) {
                    // Extract everything after the starting pattern
                    let extracted = &url[start_pos + start_pattern.len()..];
                    if !extracted.is_empty() {
                        url_replacements.insert(format!("{{{}}}", key), extracted.to_string());
                    }
                }
            }
        }
        
        // Download HTML
        let client = Client::new();

		let response = client.get(&url)
			.send()
			.map_err(|e| e.to_string())?;

		let html = response
			.text()
			.map_err(|e| e.to_string())?;
        
        // Apply filters
        let mut results = vec![html.to_string()];
        
        for filter_pair in &domain_config.filter {
            if filter_pair.len() != 2 {
                continue;
            }
            
            let start = &filter_pair[0];
            let end = &filter_pair[1];
            let mut new_results = Vec::new();
            
            for content in results {
                new_results.extend(extract_between(&content, start, end));
            }
            
            results = new_results;
        }
        
        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        results.retain(|item| seen.insert(item.clone()));
        
        // Extract URLs from results
        let mut urls = Vec::new();
        
		let mut filename_betw = std::collections::HashMap::new();
        if let Some(download_url_template) = &domain_config.download_url {
            // Use DownloadUrl template with filter results as placeholders
            for result in results {
                let trimmed_result = result.trim();
                // Skip empty results or results that look like HTML
                if !trimmed_result.is_empty() && !trimmed_result.contains('<') {
                    // Don't encode if it's already a valid filename or URL component
                    let placeholder_value = if trimmed_result.contains(|c: char| {
                        c == '?' || c == '#' || c == '[' || c == ']' || c == '@' || 
                        c == '!' || c == '$' || c == '&' || c == '\'' || c == '(' || 
                        c == ')' || c == '*' || c == '+' || c == ',' || c == ';' || 
                        c == '=' || c == '%' || c == ' ' || c == '"' || c == '<' ||
                        c == '>' || c == '{' || c == '}' || c == '|' || c == '\\' ||
                        c == '^' || c == '`' || c.is_control()
                    }) {
                        urlencoding::encode(&trimmed_result.to_string()).to_string()
                    } else {
                        trimmed_result.to_string()
                    };
                    
                    // Start with the download URL template
                    let mut download_url = download_url_template.replace("{placeholder}", &placeholder_value);
                    
                    // Replace any URL items placeholders
                    for (placeholder, value) in &url_replacements {
                        download_url = download_url.replace(placeholder, value);
                    }
					
					// Process UrlReplacesItems (replaces everything in list)
					if let Some(url_replace_items) = &domain_config.url_replace_items {
						for (key, patterns) in url_replace_items {
							if patterns.len() == 2 {
								let original = &patterns[0];
								let replace = &patterns[1];
								
								// Extract value from URL using the patterns
								if let Some(start_pos) = download_url.find(original) {
									download_url = download_url.replace(original, replace);
								}
							}
						}
					}
                    
                    println!("{}", download_url);
                    urls.push(download_url.clone());
					filename_betw.insert(download_url, trimmed_result.to_string());
                }
            }
        } else {
            // Original behavior: convert to absolute URLs
            for result in results {
                let trimmed_result = result.trim();
                if trimmed_result.starts_with("http") {
                    urls.push(trimmed_result.to_string());
					filename_betw.insert(trimmed_result.to_string(), trimmed_result.to_string());
                } else if trimmed_result.starts_with('/') {
                    // Relative URL
                    let base_url = format!("{}://{}", parsed_url.scheme(), parsed_url.host_str().unwrap());
					let download_url = format!("{}{}", base_url, trimmed_result.to_string());
                    urls.push(download_url.clone());
					filename_betw.insert(download_url, trimmed_result.to_string());
                } else if !trimmed_result.is_empty() && !trimmed_result.contains('<') {
                    // Might be a relative path without leading slash
                    let base_url = url.rsplit_once('/').map(|(base, _)| base).unwrap_or(&url);
					let download_url = format!("{}/{}", base_url, trimmed_result.to_string());
                    urls.push(download_url.clone());
					filename_betw.insert(download_url.to_string(), trimmed_result.to_string());
                }
            }
        }
        
        // Apply ignore filters
        urls.retain(|url| {
            !domain_config.ignore.iter().any(|ignore| url.contains(ignore))
        });
        
        // Create download items for all URLs
        let items: Vec<DownloadItem> = urls.into_iter()
            .map(|download_url| {
				let mut name_betw = String::new();
				for (url, path2) in &filename_betw {
					if url.to_string() == download_url.to_string() {
						name_betw = path2.to_string();
					}
				}
				
				let mut filename = extract_filename(&download_url);
				if !name_betw.is_empty() {
					filename = std::path::Path::new(&name_betw).file_name().and_then(|s| s.to_str()).unwrap_or("download").to_string();
				}
				
                DownloadItem {
                    url: download_url,
                    filename,
					name_betw,
                    progress: 0.0,
                    status: DownloadStatus::Pending,
                    total_size: 0.0,
                    downloaded: 0.0,
                }
            })
            .collect();
        
        if items.is_empty() {
            Err("No URLs found after filtering".to_string())
        } else {
            Ok(items)
        }
    } else {
        // No config for this domain, download directly
        let filename = extract_filename(&url);
		let name_betw = filename.to_string();
        Ok(vec![DownloadItem {
            url,
            filename,
			name_betw,
            progress: 0.0,
            status: DownloadStatus::Pending,
            total_size: 0.0,
            downloaded: 0.0,
        }])
    }
}

fn extract_between(content: &str, start: &str, end: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut search_from = 0;
    
    while let Some(start_pos) = content[search_from..].find(start) {
        let start_pos = search_from + start_pos + start.len();
        if let Some(end_pos) = content[start_pos..].find(end) {
            let end_pos = start_pos + end_pos;
            let extracted = content[start_pos..end_pos].trim().to_string();
            if !extracted.is_empty() {
                results.push(extracted);
            }
            search_from = end_pos + end.len();
        } else {
            break;
        }
    }
    
    results
}

fn extract_filename(url: &str) -> String {
    if let Ok(parsed) = Url::parse(url) {
        if let Some(segments) = parsed.path_segments() {
            if let Some(last) = segments.last() {
                if !last.is_empty() {
                    // Decode URL-encoded filename
                    return urlencoding::decode(last)
                        .unwrap_or(std::borrow::Cow::Borrowed(last))
                        .to_string();
                }
            }
        }
    }
    "download".to_string()
}

#[derive(Debug, Clone)]
struct ChunkInfo {
    start: u64,
    end: u64,
    chunk_num: usize,
}

async fn download_file_async(
    index: usize,
    url: String,
    filename: String,
	name_betw: String,
    output_folder: String,
    parallel: usize,
    chunk_size: usize,
    cancel_flag: Arc<AtomicBool>,
    active_downloads: Arc<Mutex<HashMap<usize, Arc<AtomicF64>>>>,
	cancel_token: Arc<tokio_util::sync::CancellationToken>, // add tokio-util = "0.7"
) -> Result<(u64, u64), String> {
    const MAX_RETRIES: u32 = 10;
    let mut retry_count = 0;
    
    loop {
		if cancel_flag.load(Ordering::Relaxed) {
			return Err("Cancelled".to_string());
		}
        match download_file_attempt(
            index,
            url.clone(),
            filename.clone(),
			name_betw.clone(),
            output_folder.clone(),
            parallel,
            chunk_size,
            cancel_flag.clone(),
            active_downloads.clone(),
			cancel_token.clone()
        ).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    eprintln!("Failed to download {} after {} retries: {}", filename, MAX_RETRIES, e);
                    return Err(format!("Failed after {} retries: {}", MAX_RETRIES, e));
                }
                
                if cancel_flag.load(Ordering::Relaxed) {
                    return Err("Cancelled".to_string());
                }
                
                eprintln!("Download failed for {}, retry {}/{}: {}", filename, retry_count, MAX_RETRIES, e);
                
                // Wait before retry (exponential backoff with max 30 seconds)
                let wait_time = std::cmp::min(30, 2u64.pow(retry_count - 1));
                tokio::time::sleep(Duration::from_secs(wait_time)).await;
            }
        }
    }
}

async fn download_file_attempt(
    index: usize,
    url: String,
    mut filename: String,
	mut name_betw: String,
    output_folder: String,
    parallel: usize,
    chunk_size: usize,
    cancel_flag: Arc<AtomicBool>,
    active_downloads: Arc<Mutex<HashMap<usize, Arc<AtomicF64>>>>,
	cancel_token: Arc<tokio_util::sync::CancellationToken>, // add tokio-util = "0.7"
) -> Result<(u64, u64), String> {
    // Ensure output directory exists
    tokio::fs::create_dir_all(&output_folder)
        .await
        .map_err(|e| format!("Failed to create output directory: {}", e))?;
    
	println!("fold {}", output_folder.to_string());
	let jp = PathBuf::from(format!("{}/{}", output_folder.to_string(), name_betw.to_string()));
    let mut output_path = jp;//PathBuf::from(&output_folder).join(&name_betw);
	if name_betw.to_string() == String::from("") {
		//output_path = PathBuf::from(&output_folder).join(&filename);
		output_path = PathBuf::from(format!("{}/{}", output_folder.to_string(), filename.to_string()));
	} else {
		filename = name_betw;
	}

	let dir = output_path.clone();
	if let Some(p) = dir.parent() {
		println!("par {} {}", output_path.display(), p.display());
		std::fs::create_dir_all(p);
	}
	let cancel_token2 = cancel_token.clone();
	let cancel_token3 = cancel_token.clone();
    // Get file size with HEAD request
    //let client = reqwest::Client::builder()
    //    .timeout(Duration::from_secs(30))
    //    .build()
    //    .map_err(|e| e.to_string())?;
	
	//let client = reqwest::Client::builder()
		//.timeout(Duration::from_secs(30))
		//.connect_timeout(Duration::from_secs(5))
		//.build()
		//.map_err(|e| format!("Client build error: {e}"))?;
    
    //let head_response = client.head(&url)
        //.send()
        //.await
        //.map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
	let mut head_response = client.head(&url).send().await.map_err(|e| format!("Request send error: {e}"))?
		.error_for_status()
		.map_err(|e| format!("HTTP error: {e}"))?;
		
    let content_length = head_response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    
    if content_length == 0 {
        return Err("Failed to get content length or file is empty".to_string());
    }
    
    // Check if file already exists with the correct size
    if output_path.exists() {
        if let Ok(metadata) = tokio::fs::metadata(&output_path).await {
            if metadata.len() == content_length {
                eprintln!("File {} already exists with correct size, skipping", filename);
                // Return special marker to indicate file was skipped
                return Ok((u64::MAX, u64::MAX));
            } else {
                eprintln!("File {} exists but size mismatch (expected: {}, actual: {}), re-downloading", 
                    filename, content_length, metadata.len());
            }
        }
    }
    
    // Create progress tracker
    let progress = Arc::new(AtomicF64::new(0.0));
    {
        let mut downloads = active_downloads.lock().await;
        downloads.insert(index, progress.clone());
    }
	let progress2 = progress.clone();
	let progress3 = progress.clone();
	let progress4 = progress.clone();
	let progress5 = progress.clone();
    
    // Check if server supports range requests
    let supports_range = head_response
        .headers()
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "bytes")
        .unwrap_or(false);
    
	let cancel_flagb = cancel_flag.clone();
	let cancel_flagc = cancel_flag.clone();
	let cancel_flagd = cancel_flag.clone();
	let cancel_flage = cancel_flag.clone();
	
	// let (cancel_tx2, mut cancel_rx2) = tokio::sync::oneshot::channel();
	
	// if cancel_flage.load(Ordering::Relaxed) {
		// let _ = cancel_tx2.send(());
	// }
	let mut written: u64 = 0;
	if !supports_range || content_length < chunk_size as u64 {
		println!("Not support range : {} content_length, {} chunk_size", content_length, chunk_size);
		// Simple download without chunking
		let url_clone = url.clone();
		let output_path_clone = output_path.clone();
		let cancel_flag_clone = cancel_flag.clone();
		let cancel_flag_clone2 = cancel_flag.clone();
		
		if cancel_flag_clone.load(Ordering::Relaxed) {
			return Err("Cancelled".to_string());
		}
		
		// Use MinReq for simple download
			
		let client = reqwest::Client::new();
		let mut resp = client.get(&url_clone).send().await.map_err(|e| format!("Request send error: {e}"))?
			.error_for_status()
			.map_err(|e| format!("HTTP error: {e}"))?;
			
		let mut file = tokio::fs::File::create(&output_path_clone).await.map_err(|e| format!("Request send error: {e}"))?;
		let mut stream = resp.bytes_stream();
		// let dest = std::fs::File::create(&output_path_clone).or_else("Create File failed".to_string())?;
		// let mut stream = response.bytes_stream();
		
		loop {
			tokio::select! {
				next = stream.next() => {
					match next {
						Some(Ok(chunk)) => {
							written += chunk.len() as u64;
							file.write_all(&chunk).await.map_err(|e| format!("Request send error: {e}"))?;
							progress.store((written as u64) as f64, Ordering::Relaxed);
						}
						Some(Err(e)) => return Err(format!("network error: {e}")),
						None => break, // finished
					}
				},
				_ = cancel_token2.cancelled() => {
					cancel_flagb.store(true, Ordering::Relaxed);
					Command::perform(
					 async {
					 },
					 |_| Message::CancelDownload);
					return Err("Cancelled".into());
				}
			}
		}
		Ok((written as u64, content_length as u64))
	} else {
		println!("Support range : {} content_length, {} chunk_size, {} parallel", content_length, chunk_size, parallel);
		// Download in chunks with resume support
		let mut chunks = Vec::new();
		let mut start = 0u64;
		let mut chunk_num = 0;
		
		while start < content_length {
			let end = std::cmp::min(start + chunk_size as u64 - 1, content_length - 1);
			chunks.push(ChunkInfo { start, end, chunk_num });
			start = end + 1;
			chunk_num += 1;
		}
		
		// Check existing chunk files and update progress
		let mut completed_chunks = Vec::new();
		let mut initial_progress = 0u64;
		
		for chunk in &chunks {
			//let chunk_path = PathBuf::from(&output_folder)
				//.join(format!("{}.tmp{}", filename, chunk.chunk_num));
			let chunk_path = PathBuf::from(format!("{}/{}.tmp{}", output_folder.to_string(), filename.to_string(), chunk.chunk_num));
			
			if chunk_path.exists() {
				if let Ok(metadata) = tokio::fs::metadata(&chunk_path).await {
					let expected_size = chunk.end - chunk.start + 1;
					if metadata.len() == expected_size {
						completed_chunks.push(chunk.chunk_num);
						initial_progress += expected_size;
						eprintln!("Resuming: chunk {} already downloaded", chunk.chunk_num);
					} else {
						// Remove incomplete chunk
						//let _ = tokio::fs::remove_file(&chunk_path).await;
					}
				}
			}
		}
		
		progress.store(initial_progress as f64, Ordering::Relaxed);
		
		// Filter out completed chunks
		let chunks_to_download: Vec<_> = chunks.iter()
			.filter(|c| !completed_chunks.contains(&c.chunk_num))
			.cloned()
			.collect();
		
		if chunks_to_download.is_empty() {
			// All chunks already downloaded, just merge
			eprintln!("All chunks already downloaded, merging...");
		} else {
			
			// Download remaining chunks
			let semaphore = Arc::new(Semaphore::new(parallel));
			let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
			
			// Create a shared counter for total downloaded bytes across all chunks
			let total_downloaded3 = Arc::new(AtomicU64::new(0));
			let mut written2: u64 = 0;
		
			let url_clone2 = url.clone();
			let cancel_token3b2 = cancel_token3.clone();
			let url_clone3 = url_clone2.clone();
			let filename2 = filename.clone();
			let output_folder22 = output_folder.clone();
			let semaphore2 = semaphore.clone();
			let cancel_flag22 = cancel_flag.clone();
			let cancel_flag32 = cancel_flag.clone();
			let cancel_flag42 = cancel_flag.clone();
			let cancel_flag2 = cancel_flag.clone();
			let progress_main2 = progress.clone();
			let total_downloaded2 = total_downloaded3.clone();
			let cancel_flagd2 = cancel_flagd.clone();
			
			println!("Download remaining chunk");
			//async move {
				let chunks_to_download2 = chunks_to_download.clone();
				let progress2a = progress2.clone();
				let content_lengtha = content_length.clone();
				//println!("{}", "Content length");
				//for chunk in chunks_to_download {
				let futures = (0..chunks_to_download2.len()).map(async |i| {
					//println!("Download chunk {}", i);
					let chunk = chunks_to_download2[i].clone();
					println!("Download chunk {}", chunk.chunk_num);
					if cancel_flag22.load(Ordering::Relaxed) {
						return Err("Cancelled".to_string());
					}
					
					let cancel_token3b = cancel_token3b2.clone();
					let url_clone4 = url_clone3.clone();
					let filename = filename2.clone();
					let output_folder2 = output_folder22.clone();
					let semaphore3 = semaphore2.clone();
					let cancel_flag2 = cancel_flag2.clone();
					let cancel_flag3 = cancel_flag2.clone();
					let cancel_flag4 = cancel_flag2.clone();
					let cancel_flag = cancel_flag2.clone();
					let progress_main = progress2a.clone();
					let total_downloaded = total_downloaded2.clone();
					let content_lengthx = content_lengtha.clone();
					let cancel_flagdd = cancel_flagd2.clone();
					let client = reqwest::Client::new();
					//let mut tasks = Vec::new();
					
					//async move {
						let permit = semaphore3.acquire().await.unwrap();
						
						//let output_pathx_clone = PathBuf::from(&output_folder2)
						//	.join(format!("{}.tmp{}", filename, chunk.chunk_num));
						let output_pathx_clone = PathBuf::from(format!("{}/{}.tmp{}", output_folder2.to_string(), filename.to_string(), chunk.chunk_num));
						
						//println!("o {}", output_pathx_clone.display());
						
						//tokio::fs::write("out.txt", output_pathx_clone.display().to_string());
						// Try MinReq first
						let chunk_start = chunk.start;
						let chunk_end = chunk.end;
						let chunk_path_clone = output_pathx_clone.clone();
						
						let expected_size = (chunk_end - chunk_start + 1) as u64;
									
						let existing_len = match std::fs::metadata(&chunk_path_clone) {
							Ok(m) => m.len().min(expected_size),
							Err(_) => 0,
						};
						//println!("{}", "aaa");
						let mut dest = tokio::fs::OpenOptions::new()
							.create(true)     // create if missing
							.write(true)      // open for write
							.truncate(false)
							.open(&chunk_path_clone)
							.await
							.map_err(|e| format!("File open error: {e}"))?;

						//println!("{}", "bbb");
						// Seek to where we left off
						dest.seek(SeekFrom::Start(existing_len)).await.map_err(|e| format!("seek: {e}"))?;

						let range_header = format!("bytes={}-{}", chunk_start + existing_len, chunk_end);
	
						println!("{}", &url_clone4);
						let mut resp = client.get(&url_clone4).header("Range", range_header).send().await.map_err(|e| format!("Request send error: {e}"))?
							.error_for_status()
							.map_err(|e| format!("HTTP error: {e}"))?;
						
						if existing_len == expected_size {
							// Already complete
							return Ok((expected_size, content_lengthx));
						}
						total_downloaded.fetch_add(existing_len, Ordering::Relaxed);
						
						let mut stream = resp.bytes_stream();
						let cc2 = cancel_flagdd.clone();
						loop {
							tokio::select! {
								next = stream.next() => {
									match next {
										Some(Ok(chunk)) => {
											total_downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
											
											dest.write_all(&chunk).await.map_err(|e| format!("Request send error: {e}"))?;
											let val = total_downloaded.load(Ordering::Relaxed);
											progress_main.store(val as f64, Ordering::Relaxed);
										}
										Some(Err(e)) => return Err(format!("network error: {e}")),
										None => break, // finished
									}
								},
								_ = cancel_token3b.cancelled() => {
									let cc = cc2.clone();
									cc.store(true, Ordering::Relaxed);
									
									Command::perform(
									 async {
									 },
									 |_| Message::CancelDownload);
									return Err("Cancelled".into());
								}
							}
						}
						 Ok((((written2 as u64) + (existing_len as u64)) as u64, content_length))
					//};
					//tasks.push(task);
					// Wait for all chunks to complete
					//let results = join_all(tasks).await;
					
					// Check for errors
					//for result in results {
					//	match result {
					//		Ok(Ok((_,_))) => {},
					//		Ok(Err(e)) => return Err(e),
					//		Err(e) => return Err(e.to_string()),
					//	}
					//}
					//Ok(())
				});
				join_all(futures).await;
			//};
			
			//let mut p = Vec::new();
			//p.push(taskx);
			//join_all(p).await;
			//taskx.join().expect("The thread panicked");
		}
		
		// Merge all chunks into final file
		if cancel_flagb.load(Ordering::Relaxed) {
			return Err("Cancelled".to_string());
		}
		
		let mut incomplete: Vec<ChunkInfo> = Vec::new();
		for chunk in &chunks {
			//let path = PathBuf::from(&output_folder)
				//.join(format!("{}.tmp{}", filename, chunk.chunk_num));
			let path = PathBuf::from(format!("{}/{}.tmp{}", output_folder.to_string(), filename.to_string(), chunk.chunk_num));

			let expected_size = chunk.end - chunk.start + 1;
			match tokio::fs::metadata(&path).await {
				Ok(meta) if meta.len() == expected_size => {
					// ok
				}
				Ok(meta) => {
					// short or too long – treat as incomplete; we will resume from meta.len()
					incomplete.push(chunk.clone());
				}
				Err(_) => {
					// missing entirely
					incomplete.push(chunk.clone());
				}
			}
		}
		
		if incomplete.len() == 0 {
			let mut final_file = File::create(&output_path)
				.await
				.map_err(|e| format!("Failed to create final file: {}", e))?;
			
			for chunk_num in 0..chunks.len() {
				// Merge all chunks into final file
				if cancel_flagc.load(Ordering::Relaxed) {
					return Err("Cancelled".to_string());
				}
				
				//let chunk_path = PathBuf::from(&output_folder)
					//.join(format!("{}.tmp{}", filename, chunk_num));
				let chunk_path = PathBuf::from(format!("{}/{}.tmp{}", output_folder.to_string(), filename.to_string(), chunk_num));
				
				let mut chunk_file = File::open(&chunk_path)
					.await
					.map_err(|e| format!("Failed to open chunk file: {}", e))?;
				
				let mut buffer = vec![0u8; 8 * 1024 * 1024]; // 8 MB buffer
				loop {
					// Read up to 8MB into the buffer
					if let Ok(n) = chunk_file
						.read(&mut buffer)
						.await
						.map_err(|e| format!("Failed to read chunk file: {}", e)) {

						if n == 0 {
							break; // EOF reached
						}

						// Write only the bytes actually read
						if let Ok(m) = final_file
							.write_all(&buffer[..n])
							.await
							.map_err(|e| format!("Failed to write to final file: {}", e)) {
						} else {
							return Err("Error during merge file: write".to_string());
						}
					} else {
						return Err("Error during merge file: read".to_string());
					}
				}
			}
			
			final_file.flush().await.map_err(|e| format!("Failed to flush file: {}", e))?;
			drop(final_file);
			
			// Delete temporary chunk files
			for chunk_num in 0..chunks.len() {
				if cancel_flagd.load(Ordering::Relaxed) {
					return Err("Cancelled".to_string());
				}
				
				//let chunk_path = PathBuf::from(&output_folder)
				//	.join(format!("{}.tmp{}", filename, chunk_num));
				let chunk_path = PathBuf::from(format!("{}/{}.tmp{}", output_folder.to_string(), filename.to_string(), chunk_num));
				
				let _ = tokio::fs::remove_file(&chunk_path).await;
			}
			
			// Ensure progress shows completion
			progress.store(content_length as f64, Ordering::Relaxed);
			
			Ok((content_length, content_length))
		} else {
			return Err("Incomplete download".to_string());
		}
	}
}