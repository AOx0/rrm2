#![feature(let_chains)]

use std::{path::PathBuf, str::FromStr};
mod steam_cmd;

#[tokio::main]
async fn main() {
    let steam_cmd::Handle { mut events } = steam_cmd::Steam::builder()
        .home(PathBuf::from_str("/home/ae/configs/\"      \"").unwrap())
        .exe(PathBuf::from_str("/home/ae/.config/rrm/steamcmd/steamcmd.sh").unwrap())
        .add_item(steam_cmd::Item {
            game: steam_cmd::GameId(294100),
            item: steam_cmd::ItemId(1631756268),
        })
        .add_item(steam_cmd::Item {
            game: steam_cmd::GameId(294100),
            item: steam_cmd::ItemId(1631756268),
        })
        .add_item(steam_cmd::Item {
            game: steam_cmd::GameId(294100),
            item: steam_cmd::ItemId(1631756268),
        })
        .build()
        .unwrap()
        .spawn()
        .await
        .unwrap();

    while let Some(event) = events.recv().await {
        println!("{event:?}");
    }
}
