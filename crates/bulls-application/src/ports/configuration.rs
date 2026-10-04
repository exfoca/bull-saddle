// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use super::PortResult;

pub trait ConfigurationPort {
    type Configuration;

    fn load(&self) -> PortResult<Option<Self::Configuration>>;

    fn save(&mut self, configuration: &Self::Configuration) -> PortResult<()>;
}
