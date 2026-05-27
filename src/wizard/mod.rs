// This file is part of Sirup, a CLI to spawn pre-authenticated IMAP/SMTP
// sessions and expose them via Unix sockets.
//
// Copyright (C) 2026  soywod <pimalaya.org@posteo.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! In-memory, first-run wizard: takes one URL/email/domain input,
//! probes PACC → Autoconfig → SRV for the server endpoints, then
//! prompts for SASL credentials. Returns a single in-memory
//! [`crate::config::AccountConfig`]; no on-disk config is written.

pub mod autoconfig;
pub mod discover;
pub mod pacc;
pub mod srv;
