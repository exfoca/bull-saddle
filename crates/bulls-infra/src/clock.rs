// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use bulls_application::ports::ClockPort;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeClock;

impl NativeClock {
    pub const fn new() -> Self {
        Self
    }
}

impl ClockPort for NativeClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}
