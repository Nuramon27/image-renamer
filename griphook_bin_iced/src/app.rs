use std::cmp;
use std::collections::{HashMap, HashSet, VecDeque};
use std::iter;
use std::path::PathBuf;
use std::borrow::Cow;

use clap::Parser;
use iced::Alignment;
use iced::Color;
use iced::{
    Subscription,
    ContentFit, Element, Length, Task, Theme, keyboard, widget::{Column, Space, Text, button, column, container, image, row, scrollable, text, text_input},
};

use griphook_logic::{
    files::{DEFAULT_PARSER, DEFAULT_REPLACEMENT, FileError, ImageDirectory, ImageFile, RenameOperation, rename::RenameError}, preview,
};
use crate::opt::Opt;

#[derive(Debug, Clone, Copy, Default)]
struct ModifierState {
    ctrl: bool,
    shift: bool,
}

#[derive(Default)]
struct FileState {
    directory: PathBuf,
    files: Vec<ImageFile>,
    filter: String,
    displayed: Option<usize>,
    status: Option<FileError>
}

struct PreviewState {
    preview: Option<image::Handle>,
    old_displayed_file: Option<PathBuf>,
    status: Result<Option<String>, String>,
}

/// Thumbnails that have been decoded, are waiting to be decoded, or are in flight.
struct ThumbnailState {
    cache: HashMap<PathBuf, image::Handle>,
    queued: VecDeque<PathBuf>,
    loading: HashSet<PathBuf>,
    discarded: HashSet<PathBuf>,
    max_loading: usize,
}

impl Default for ThumbnailState {
    fn default() -> Self {
        let max_loading = std::thread::available_parallelism()
            .map(|threads| (threads.get() / 3).max(1))
            .unwrap_or(1);
        Self { cache: HashMap::new(), queued: VecDeque::new(), loading: HashSet::new(), discarded: HashSet::new(), max_loading }
    }
}

impl Default for PreviewState {
    fn default() -> Self {
        PreviewState {
            preview: None,
            old_displayed_file: None,
            status: Ok(None),
        }
    }
}

enum GriphookError {
    OnRename(RenameError)
}

#[derive(Default)]
pub struct App {
    file: FileState,
    preview: PreviewState,
    thumbnails: ThumbnailState,
    replacement: String,
    set_name: String,
    busy: bool,
    modifiers: ModifierState,
    error_status: Option<GriphookError>
}

#[derive(Debug, Clone)]
pub enum Message {
    FilesLoaded(Result<Vec<ImageFile>, FileError>),
    FilesRefreshed(Result<Vec<ImageFile>, FileError>, Option<PathBuf>),
    FilterChanged(String),
    ReplacementChanged(String),
    SetNameChanged(String),
    Toggle(usize),
    PreviewLoaded(Result<Vec<u8>, String>),
    ThumbnailLoaded(PathBuf, Result<Vec<u8>, String>),
    Scrolled { offset_y: f32, height: f32 },
    Rename,
    Renamed(Result<Vec<(PathBuf, PathBuf)>, RenameError>),
    FileClicked(usize),
    ModifiersChanged {
        ctrl: bool,
        shift: bool,
    }
}

impl App {
    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen()
            .filter_map(|event| match event {
                keyboard::Event::ModifiersChanged(modifiers) => {
                    Some(Message::ModifiersChanged { ctrl: modifiers.control(), shift: modifiers.shift() })
                },
                _ => None
            })
    }
    pub fn new() -> (Self, Task<Message>) {
        let args = Opt::parse();
        let directory = args.dir.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let app = Self {
            file: FileState {
                directory: directory.clone(),
                filter: DEFAULT_PARSER.into(),
                ..Default::default()
            },
            replacement: DEFAULT_REPLACEMENT.into(),
            ..Self::default()
        };
        (app, Self::load_files(directory, DEFAULT_PARSER.into()))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::FilesLoaded(result) => {
                self.busy = false;
                match result {
                    Ok(files) => {
                        self.file.status = None;
                        self.file.files = files;
                        return self.schedule_thumbnails(0, 16);
                    }
                    Err(error) => self.file.status = Some(error),
                }
            }
            Message::FilesRefreshed(result, displayed_path) => {
                self.busy = false;
                match result {
                    Ok(files) => {
                        self.file.files = files;
                        self.file.displayed = displayed_path
                            .as_ref()
                            .and_then(|path| self.file.files.iter().position(|file| &file.path == path));
                        self.file.status = None;
                        let thumbnails = self.schedule_thumbnails(0, 16);
                        let preview = self.file.displayed
                            .map(|index| Self::load_preview(self.file.files[index].path.clone()));
                        return Task::batch(iter::once(thumbnails).chain(preview));
                    }
                    Err(error) => self.file.status = Some(error),
                }
            }
            Message::FilterChanged(filter) => {
                self.file.filter = filter;
                self.busy = true;
                return Self::load_files(self.file.directory.clone(), self.file.filter.clone());
            }
            Message::ReplacementChanged(replacement) => self.replacement = replacement,
            Message::SetNameChanged(set_name) => self.set_name = set_name,
            Message::Toggle(index) => {
                if let Some(file) = self.file.files.get_mut(index) {
                    file.selected = !file.selected;
                }
            }
            Message::PreviewLoaded(result) => match result {
                Ok(bytes) => {
                    self.preview.preview = Some(image::Handle::from_bytes(bytes));
                    self.preview.old_displayed_file = None;
                    self.preview.status = Ok(None);
                }
                Err(error) => self.preview.status = Err(error),
            },
            Message::ThumbnailLoaded(path, result) => {
                self.thumbnails.loading.remove(&path);
                if !self.thumbnails.discarded.remove(&path) && let Ok(bytes) = result {
                    self.thumbnails.cache.insert(path, image::Handle::from_bytes(bytes));
                }
                if !self.busy {
                    return self.start_thumbnail_loads();
                }
            },
            Message::Scrolled { offset_y, height } => {
                let first = (offset_y / Self::FILE_ROW_HEIGHT).floor().max(0.0) as usize;
                let visible = (height / Self::FILE_ROW_HEIGHT).ceil() as usize + 2;
                return self.schedule_thumbnails(first, visible);
            },
            Message::Rename => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.thumbnails.queued.clear();
                self.thumbnails.discarded.extend(self.thumbnails.loading.clone());
                if let Some(displayed_file) = &self.file.displayed {
                    self.preview.old_displayed_file = Some(self.file.files[*displayed_file].path.clone())
                }
                return Self::rename_files(
                    self.file.files.clone(),
                    self.file.filter.clone(),
                    self.replacement.clone(),
                    self.set_name.clone(),
                );
            }
            Message::Renamed(result) => {
                self.busy = false;
                match result {
                    Ok(renamed) => {
                        for (from, to) in &renamed {
                            if let Some(thumbnail) = self.thumbnails.cache.remove(from) {
                                self.thumbnails.cache.insert(to.clone(), thumbnail);
                            }
                        }
                        let displayed_path = self
                            .file
                            .displayed
                            .and_then(|index| self.file.files.get(index))
                            .map(|file| file.path.clone());
                        let displayed_path = displayed_path.and_then(|path| {
                            renamed
                                .iter()
                                .find(|(from, _)| from == &path)
                                .map(|(_, to)| to.clone())
                                .or(Some(path))
                        });
                        let load_preview_task = if let Some(old_displayed_file) = self.preview.old_displayed_file.take() {
                            if let Some((_, new_displayed_file)) = renamed.iter().find(|(f, _)| f == &old_displayed_file) {
                                Some(Self::load_preview(new_displayed_file.to_owned()))
                            } else {
                                Some(Self::load_preview(old_displayed_file.to_owned()))
                            }
                        } else {
                            None
                        };
                        self.error_status = None;
                        self.busy = true;
                        let directory = self.file.directory.clone();
                        let filter = self.file.filter.clone();
                        self.file.status = None;
                        return Task::batch(load_preview_task.into_iter().chain(iter::once(Task::perform(
                            async move { ImageDirectory::new(directory).scan(&filter) },
                            move |result| Message::FilesRefreshed(result, displayed_path),
                        ))));
                    }
                    Err(err) => {
                        self.error_status = Some(GriphookError::OnRename(err));
                        return self.schedule_thumbnails(0, 16);
                    },
                }
            },
            Message::FileClicked(index) => {
                if self.modifiers.ctrl {
                    if self.modifiers.shift {
                        if let Some(displayed) = self.file.displayed {
                            let lower = cmp::min(displayed, index);
                            let upper = cmp::max(displayed, index);
                            for ix in lower..=upper {
                                if let Some(file) = self.file.files.get_mut(ix) {
                                    file.selected = !file.selected;
                                }
                            }
                        }
                    } else {
                        if let Some(file) = self.file.files.get_mut(index) {
                            file.selected = !file.selected;
                        }
                    }
                } else {
                    self.file.displayed = Some(index);
                    self.preview.preview = None;
                    self.preview.status = Ok(Some("Loading preview…".to_string()));
                    return Self::load_preview(self.file.files[index].path.clone());
                }
            },
            Message::ModifiersChanged{ ctrl, shift } => {
                self.modifiers.ctrl = ctrl;
                self.modifiers.shift = shift;
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let filters = row![
            text_input("Filter regular expression", &self.file.filter)
                .on_input(Message::FilterChanged)
                .width(Length::FillPortion(1)),
            text_input("Replacement", &self.replacement)
                .on_input(Message::ReplacementChanged)
                .width(Length::FillPortion(1)),
        ]
        .spacing(10);
        let mut list = Column::new().spacing(2);
        for (index, file) in self.file.files.iter().enumerate() {
            let name = file
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("?");
            let label = if let Some(thumbnail) = self.thumbnails.cache.get(&file.path) {
                row![
                    image(thumbnail.clone()).width(36).height(36).content_fit(ContentFit::Cover),
                    text(name),
                ].spacing(4)
            } else {
                row![text(name)]
            };
            list = list.push(
                row![
                    button(label)
                        .on_press(Message::FileClicked(index))
                        .style(move |theme, status| Self::button_highlighting(file.selected, self.file.displayed == Some(index), theme, status))
                        .width(Length::Fill),
                    button("✓").on_press(Message::Toggle(index)),
                ]
                .spacing(4),
            );
        }
        let list = scrollable(list)
            .spacing(4)
            .width(Length::Fill)
            .height(Length::Fill)
            .on_scroll(|viewport| Message::Scrolled {
                offset_y: viewport.absolute_offset().y,
                height: viewport.bounds().height,
            });
        let list_area = if let Some((color, err_message)) = self.error_message() {
            let error_text = container(Text::new(err_message)
                    .color(Color::from_rgb8(0xe1, 0xe5, 0xef))
                    .align_x(Alignment::Center))
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(color)),
                    ..Default::default()
                });
            column![
                list,
                error_text
            ]
        } else {
            column![
                list
            ]
        };
        let preview: Element<'_, _> = match &self.preview.preview {
            Some(handle) => image::Viewer::new(handle.clone())
                .content_fit(ContentFit::Contain)
                .min_scale(0.25)
                .max_scale(10.0)
                .filter_method(image::FilterMethod::Linear)
                .scale_step(0.10)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => container(text(match &self.preview.status {
                Ok(None) => Cow::Borrowed("Select an image"),
                Ok(Some(status)) => Cow::Borrowed(status.as_str()),
                Err(err) => Cow::Owned(format!("Error: {}", err))
            }))
                .center(Length::Fill)
                .into(),
        };
        let content = row![
            list_area.width(Length::FillPortion(2)),
            container(preview)
                .width(Length::FillPortion(3))
                .height(Length::Fill),
        ]
            .spacing(10)
            .height(Length::Fill);
        column![
            filters,
            text_input("Set name", &self.set_name).on_input(Message::SetNameChanged),
            content,
            row![
                Space::new().width(Length::Fill),
                button(if self.busy { "Working…" } else { "Rename" })
                    .on_press_maybe((!self.busy).then_some(Message::Rename)),
            ]
            .spacing(10),
        ]
        .padding(12)
        .spacing(10)
        .into()
    }

    pub fn theme(_: &App) -> Theme {
        Theme::Dark
    }

    fn load_files(directory: PathBuf, filter: String) -> Task<Message> {
        Task::perform(
            async move { ImageDirectory::new(directory).scan(&filter) },
            Message::FilesLoaded,
        )
    }

    fn rename_files(
        files_to_rename: Vec<ImageFile>,
        parser: String,
        replacement: String,
        set_name: String,
    ) -> Task<Message> {
        Task::perform(
            async move {
                RenameOperation {
                    files: &files_to_rename,
                    parser: &parser,
                    replacement: &replacement,
                    set_name: &set_name
                }
                .execute()
            },
            Message::Renamed,
        )
    }

    fn load_preview(path: PathBuf) -> Task<Message> {
        Task::perform(
            // TODO: Check if this might be the file already open.
            async move { preview::load(&path) },
            Message::PreviewLoaded,
        )
    }

    const FILE_ROW_HEIGHT: f32 = 42.0;

    /// Queues visible files first, leaving queued off-screen work available for later.
    fn schedule_thumbnails(&mut self, first: usize, visible: usize) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        let paths = self.file.files.iter().skip(first).take(visible)
            .map(|file| file.path.clone()).collect::<Vec<_>>();
        for path in paths.into_iter().rev() {
            if !self.thumbnails.cache.contains_key(&path)
                && !self.thumbnails.loading.contains(&path)
                && !self.thumbnails.queued.contains(&path)
            {
                self.thumbnails.queued.push_front(path);
            }
        }
        self.start_thumbnail_loads()
    }

    /// Starts only the bounded number of thumbnail tasks allowed by this application.
    fn start_thumbnail_loads(&mut self) -> Task<Message> {
        if self.busy {
            return Task::none();
        }
        let mut tasks = Vec::new();
        while self.thumbnails.loading.len() < self.thumbnails.max_loading {
            let Some(path) = self.thumbnails.queued.pop_front() else { break };
            self.thumbnails.loading.insert(path.clone());
            tasks.push(Self::load_thumbnail(path));
        }
        Task::batch(tasks)
    }

    fn load_thumbnail(path: PathBuf) -> Task<Message> {
        Task::perform(
            async move {
                let result = griphook_logic::thumbnail::load(&path);
                (path, result)
            },
            |(path, result)| Message::ThumbnailLoaded(path, result),
        )
    }

    fn error_message(&self) -> Option<(Color, String)> {
        if let Some(own_err) = &self.error_status {
            match own_err {
                GriphookError::OnRename(err) => Some((Color::from_rgb8(0x80, 0x15, 0x15), err.to_string()))
            }
        } else if let Some(file_err) = &self.file.status {
            Some((Color::from_rgb8(0x80, 0x6d, 0x15), file_err.to_string()))
        } else {
            None
        }
    }

    fn button_highlighting(selected: bool, displayed: bool, theme: &Theme, status: button::Status) -> button::Style {
        use iced::Background::Color;
        match status {
            button::Status::Active
            | button::Status::Hovered => match (displayed, selected) {
                (false, false) => button::Style {
                    background: Some(Color(theme.extended_palette().background.base.color)),
                    text_color: theme.extended_palette().background.base.text,
                    ..button::Style::default()
                },
                (false, true) => button::Style {
                    background: Some(Color(theme.extended_palette().background.strong.color)),
                    text_color: theme.extended_palette().background.strong.text,
                    ..button::Style::default()
                },
                (true, false) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.weak.color)),
                    text_color: theme.extended_palette().primary.base.text,
                    ..button::Style::default()
                },
                (true, true) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.base.color)),
                    text_color: theme.extended_palette().primary.strong.text,
                    ..button::Style::default()
                },
            },
            button::Status::Disabled => button::Style {
                background: Some(Color(theme.extended_palette().background.weak.color)),
                text_color: theme.extended_palette().background.weak.text,
                ..button::Style::default()
            },
            button::Status::Pressed => match (displayed, selected) {
                (false, false) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.base.color)),
                    text_color: theme.extended_palette().primary.base.text,
                    ..button::Style::default()
                },
                (false, true) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.strong.color)),
                    text_color: theme.extended_palette().primary.strong.text,
                    ..button::Style::default()
                },
                (true, false) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.base.color)),
                    text_color: theme.extended_palette().primary.base.text,
                    ..button::Style::default()
                },
                (true, true) => button::Style {
                    background: Some(Color(theme.extended_palette().primary.strong.color)),
                    text_color: theme.extended_palette().primary.strong.text,
                    ..button::Style::default()
                },
            },
        }

    }
}
