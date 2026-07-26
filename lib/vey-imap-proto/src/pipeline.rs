/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use ahash::AHashMap;
use smol_str::SmolStr;

use crate::command::Command;
use crate::response::UntaggedResponse;

pub struct CommandPipeline {
    cached_commands: AHashMap<SmolStr, Command>,
    ongoing_command: Option<Command>,
    ongoing_response: Option<UntaggedResponse>,
}

impl Default for CommandPipeline {
    fn default() -> Self {
        CommandPipeline::new()
    }
}

impl CommandPipeline {
    pub fn new() -> Self {
        CommandPipeline::with_capacity(32)
    }

    pub fn with_capacity(cap: usize) -> Self {
        CommandPipeline {
            cached_commands: AHashMap::with_capacity(cap),
            ongoing_command: None,
            ongoing_response: None,
        }
    }

    pub fn insert_completed(&mut self, cmd: Command) -> Option<Command> {
        let tag = cmd.tag.clone();
        self.cached_commands.insert(tag, cmd)
    }

    pub fn remove(&mut self, tag: &SmolStr) -> Option<Command> {
        if let Some(cmd) = self.cached_commands.remove(tag) {
            return Some(cmd);
        };
        if let Some(cmd) = self.ongoing_command.take() {
            if cmd.tag.eq(tag) {
                return Some(cmd);
            } else {
                self.ongoing_command = Some(cmd);
            }
        }
        None
    }

    pub fn set_ongoing_command(&mut self, cmd: Command) {
        self.ongoing_command = Some(cmd);
    }

    pub fn ongoing_command(&mut self) -> Option<&mut Command> {
        self.ongoing_command.as_mut()
    }

    pub fn take_ongoing_command(&mut self) -> Option<Command> {
        self.ongoing_command.take()
    }

    pub fn set_ongoing_response(&mut self, rsp: UntaggedResponse) {
        self.ongoing_response = Some(rsp);
    }

    pub fn ongoing_response(&mut self) -> Option<&mut UntaggedResponse> {
        self.ongoing_response.as_mut()
    }

    pub fn take_ongoing_response(&mut self) -> Option<UntaggedResponse> {
        self.ongoing_response.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, ParsedCommand};
    use crate::response::{CommandData, UntaggedResponse};

    fn tagged_command(tag: &str) -> Command {
        Command {
            tag: SmolStr::from(tag),
            parsed: ParsedCommand::NoOperation,
            literal_arg: None,
        }
    }

    fn empty_untagged() -> UntaggedResponse {
        UntaggedResponse {
            command_data: CommandData::Other,
            literal_data: None,
        }
    }

    #[test]
    fn insert_and_remove_completed() {
        let mut pipeline = CommandPipeline::new();
        let old = pipeline.insert_completed(tagged_command("A001"));
        assert!(old.is_none());
        assert!(pipeline.remove(&SmolStr::from("A001")).is_some());
        assert!(pipeline.remove(&SmolStr::from("A001")).is_none());
    }

    #[test]
    fn remove_ongoing_command() {
        let mut pipeline = CommandPipeline::new();
        pipeline.set_ongoing_command(tagged_command("B002"));
        assert!(pipeline.remove(&SmolStr::from("B002")).is_some());

        pipeline.set_ongoing_command(tagged_command("B003"));
        assert!(pipeline.remove(&SmolStr::from("B002")).is_none());
        assert!(pipeline.ongoing_command().is_some());
    }

    #[test]
    fn ongoing_response_lifecycle() {
        let mut pipeline = CommandPipeline::new();
        assert!(pipeline.ongoing_response().is_none());

        pipeline.set_ongoing_response(empty_untagged());
        assert!(pipeline.ongoing_response().is_some());
        assert!(pipeline.take_ongoing_response().is_some());
        assert!(pipeline.take_ongoing_response().is_none());
    }
}
