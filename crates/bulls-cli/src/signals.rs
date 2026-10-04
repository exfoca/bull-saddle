// BullSaddle — Developed by Orion Impact (https://orion-impact.com)
// Licensed under the Apache License, Version 2.0.
// SPDX-License-Identifier: Apache-2.0

use bulls_runtime::RuntimeCancellationHandle;

type InterruptHandler = Box<dyn FnMut() + Send + 'static>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SignalHandlerError;

pub(crate) fn install_cancellation_handler(
    cancellation: RuntimeCancellationHandle,
) -> Result<(), SignalHandlerError> {
    install_with(cancellation, ctrlc::set_handler)
}

fn install_with<E>(
    cancellation: RuntimeCancellationHandle,
    installer: impl FnOnce(InterruptHandler) -> Result<(), E>,
) -> Result<(), SignalHandlerError> {
    installer(Box::new(move || cancellation.cancel())).map_err(|_| SignalHandlerError)
}

#[cfg(test)]
mod tests {
    use bulls_runtime::RuntimeCancellationHandle;

    use super::{SignalHandlerError, install_with};

    #[test]
    fn installed_handler_only_requests_runtime_cancellation() {
        let cancellation = RuntimeCancellationHandle::default();
        let observed = cancellation.clone();

        install_with(cancellation, |mut handler| {
            assert!(!observed.is_cancelled());
            handler();
            Ok::<(), ()>(())
        })
        .expect("test handler installation must succeed");

        assert!(observed.is_cancelled());
    }

    #[test]
    fn installation_failure_is_reduced_to_a_private_free_error() {
        let cancellation = RuntimeCancellationHandle::default();
        let error = install_with(cancellation, |_handler| Err::<(), _>("private detail"))
            .expect_err("installation failure must be surfaced");

        assert_eq!(error, SignalHandlerError);
    }
}
