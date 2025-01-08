use core::panic;
use std::{path::PathBuf, process::Stdio, str::FromStr};

use derive_builder::Builder;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

pub struct Handle {
    pub events: mpsc::Receiver<Event>,
}

#[derive(Debug, Clone, Copy)]
pub struct GameId(pub usize);

#[derive(Debug, Clone, Copy)]
pub struct ItemId(pub usize);

#[derive(Debug)]
pub enum OutputLine {
    Error(String),
    Normal(String),
}

#[derive(Debug)]
pub enum Event {
    /// A line from stdout or stderr from the process
    Output(OutputLine),
    /// The ItemId that began downloading
    Starting(ItemId),
    /// The ItemId, the path were it was downloaded and the number of bytes
    Done(ItemId, PathBuf, usize),
}

#[derive(Clone, Copy)]
pub struct Item {
    pub game: GameId,
    pub item: ItemId,
}

#[derive(Builder)]
pub struct Steam {
    /// Where the steamcmd should place its directory relative to
    home: PathBuf,
    /// Where the steamcmd binary is located at
    exe: PathBuf,
    /// Items to download
    #[builder(setter(custom))]
    items: Vec<Item>,
}

impl SteamBuilder {
    pub fn add_item(&mut self, item: Item) -> &mut SteamBuilder {
        let items = if let Some(ref mut items) = self.items {
            items
        } else {
            self.items = Some(vec![]);
            self.items.as_mut().unwrap()
        };

        items.push(item);

        self
    }
}

impl Steam {
    pub fn builder() -> SteamBuilder {
        SteamBuilder::default()
    }

    pub async fn spawn(self) -> Result<Handle, std::io::Error> {
        let (tx, rx) = mpsc::channel(100);

        #[allow(unused_mut)]
        let mut command: Command;

        #[cfg(target_os = "windows")]
        {
            command = Command::new(self.exe);
        }

        #[cfg(not(target_os = "windows"))]
        {
            command = Command::new("env");
            command.arg(format!("HOME={home}", home = self.home.display()));
            command.arg(self.exe);
        }

        command.current_dir(self.home);

        command.args(["+login", "anonymous"]);

        let mut game_id_buff = [0u8; 25];
        let mut item_id_buff = [0u8; 25];

        for Item {
            game: GameId(game_id),
            item: ItemId(item_id),
        } in self.items
        {
            let game_id = write_number_into_buff(&mut game_id_buff, game_id);
            let item_id = write_number_into_buff(&mut item_id_buff, item_id);

            command
                .arg("+workshop_download_item")
                .arg(game_id)
                .arg(item_id);
        }

        command.arg("+quit");
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        let mut child = command.spawn()?;

        let stdout = child
            .stdout
            .take()
            .expect("Taking stdout should not fail since its performed only here");

        let stderr = child
            .stderr
            .take()
            .expect("Taking stderr should not fail since its performed only here");

        // Spawn the stdout handler
        tokio::spawn({
            let tx = tx.clone();

            async move {
                let mut lines = BufReader::new(stdout).lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    let mut words = line.split(' ').peekable();

                    while let Some(word) = words.next() {
                        if word.trim() == "Downloading"
                            && words.peek().is_some_and(|w| w == &"item")
                        {
                            handle_download_start(&tx, &mut words).await;
                        } else if word.trim() == "Downloaded"
                            && words.peek().is_some_and(|w| w == &"item")
                        {
                            handle_download_end(&tx, &mut words).await;
                        }
                    }

                    _ = tx.send(Event::Output(OutputLine::Normal(line))).await;
                }
            }
        });

        // Spawn the stderr handler
        tokio::spawn({
            let tx = tx.clone();

            async move {
                let mut lines = BufReader::new(stderr).lines();

                while let Ok(Some(line)) = lines.next_line().await {
                    _ = tx.send(Event::Output(OutputLine::Error(line))).await;
                }
            }
        });

        Ok(Handle { events: rx })
    }
}

async fn handle_download_end(
    tx: &mpsc::Sender<Event>,
    words: &mut std::iter::Peekable<std::str::Split<'_, char>>,
) {
    _ = words.next();
    // Skip "item"
    let item_id = words
        .next()
        .map(|id| {
            id.trim()
                .parse::<usize>()
                .expect("Steam should always provide valid Item IDs")
        })
        .expect("Expected \"Downloaded item ITEM_ID\"");

    _ = words.next();
    // Skip "to"

    let mut path = words
        .next()
        .expect("Expected \"Downlaoded item ITEM_ID to \"PATH\"")
        .to_string();

    let size = loop {
        let Some(curr) = words.next() else {
            panic!("Never reached \"(BYTES bytes)\" in stdout");
        };
        let next = words.peek();

        if next.is_some_and(|next| next.trim() == "bytes)") {
            let bytes = curr
                .trim_start_matches("(")
                .parse::<usize>()
                .expect("Steamcmd should always report valid bytes size");

            words.next(); // Skip "bytes)"

            break bytes;
        } else {
            path += " ";
            path += curr;
        }
    };

    let path = PathBuf::from_str(&path[1..path.len() - 1]).unwrap();

    _ = tx.send(Event::Done(ItemId(item_id), path, size)).await;
}

async fn handle_download_start(
    tx: &mpsc::Sender<Event>,
    words: &mut std::iter::Peekable<std::str::Split<'_, char>>,
) {
    _ = words.next();
    // Skip "item"
    let item_id = words
        .next()
        .map(|id| {
            id.trim()
                .parse::<usize>()
                .expect("Steam should always provide valid Item IDs")
        })
        .expect("Expected \"Downloading item ITEM_ID\"");

    _ = tx.send(Event::Starting(ItemId(item_id))).await
}

fn write_number_into_buff(buff: &mut [u8], value: usize) -> &str {
    use std::io::{Cursor, Write};

    let mut cursor = Cursor::new(buff);

    write!(cursor, "{value}").unwrap();
    let pos = cursor.position();
    let buffer = cursor.into_inner();

    std::str::from_utf8(&buffer[..pos as usize]).unwrap()
}
