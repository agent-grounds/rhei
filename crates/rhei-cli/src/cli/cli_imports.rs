// Shared imports for the CLI include modules; command declarations own
// only their clap schema. §AR-source-file-size.3

use anyhow::{Context, Result};
use clap::{error::ErrorKind, Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::{
    ArgValueCompleter, CompletionCandidate, PathCompleter, ValueCompleter,
};
use clap_complete::env::{
    Bash as CompletionBash, Elvish as CompletionElvish, EnvCompleter, Fish as CompletionFish,
    Powershell as CompletionPowerShell, Zsh as CompletionZsh,
};
use clap_complete::CompleteEnv;
use fs2::FileExt;
use indexmap::IndexMap;
use miette::{miette, Report, Result as MietteResult};
use minijinja::{Environment as MiniJinjaEnvironment, UndefinedBehavior};
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use rhei_core::ast::{Metadata, TaskId};
use rhei_core::callback::{CallbackContext, CallbackExecutor, ShellCallbackExecutor};
use rhei_core::workspace;
use rhei_validator::{
    parse_execution_target, AgentConfig, CustomAgentProfile, ExecutionTarget, McpServerProfile,
    SkillProfile, StateMcpEntry, StateMcpEntryObject, StateSkillEntry,
};
use serde::Deserialize;
use serde_yaml::{Mapping as YamlMapping, Value as YamlValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
