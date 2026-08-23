use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone)]
#[derive(Parser)]
pub struct Opt {
    #[arg(short, long)]
    pub dir: Option<PathBuf>
}