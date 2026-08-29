use std::cmp;
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
    replacement: String,
    set_name: String,
    busy: bool,
    modifiers: ModifierState,
    error_status: Option<GriphookError>
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowKey {
    Up,
    Down
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
    Rename,
    Renamed(Result<Vec<(PathBuf, PathBuf)>, RenameError>),
    FileClicked(usize),
    ArrowKey(i8),
    ModifiersChanged {
        ctrl: bool,
        shift: bool,
    }
}

impl App {
    pub fn subscription(&self) -> Subscription<Message> {
        keyboard::listen()
            .filter_map(|event| match event {
                keyboard::Event::KeyPressed { key, .. } => {
                    match key {
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowDown) => {
                            Some(Message::ArrowKey(1))
                        },
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::ArrowUp) => {
                            Some(Message::ArrowKey(-1))
                        },
                        _ => None,
                    }
                }
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
                        if let Some(index) = self.file.displayed {
                            return Self::load_preview(self.file.files[index].path.clone());
                        }
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
            Message::Rename => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
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
                    Err(err) => self.error_status = Some(GriphookError::OnRename(err)),
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
            Message::ArrowKey(direction) => {
                if let Some(displayed) = &mut self.file.displayed {
                    if direction < 0 {
                        *displayed = displayed.saturating_sub(direction.abs() as usize)
                    } else if direction > 0 {
                        *displayed = cmp::min(
                            displayed.saturating_add(direction.abs() as usize), self.file.files.len()-1
                        );
                    }
                    return Self::load_preview(self.file.files[*displayed].path.clone());
                }
            }
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
            list = list.push(
                row![
                    button(text(name))
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
            .height(Length::Fill);
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
