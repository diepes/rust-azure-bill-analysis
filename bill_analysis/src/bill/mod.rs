use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs::File;
use std::hash::Hash;
use std::path::{Path, PathBuf};

// 1brc speedup
use std::time::Instant;
pub mod calc;
pub mod tags;
pub mod billentry;
pub mod costtype;
pub mod bills;
