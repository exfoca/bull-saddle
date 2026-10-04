// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

pub trait ClockPort: Sync {
    fn now(&self) -> SystemTime;
}
