// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_core::{Observation, ObservationAttempt, ObservationKey};

use super::PortResult;

pub trait ObservationStorePort {
    type Value;

    fn latest_observation(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<Observation<Self::Value>>>;

    fn latest_attempt(
        &self,
        key: &ObservationKey,
    ) -> PortResult<Option<ObservationAttempt<Self::Value>>>;

    fn save_attempt(&mut self, attempt: &ObservationAttempt<Self::Value>) -> PortResult<()>;
}
