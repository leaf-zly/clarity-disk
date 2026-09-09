//! Prevents updater-triggered process exit during tracked application operations.

use std::sync::Mutex;

#[derive(Default)]
struct State {
    active: usize,
    installing: bool,
}

/// Process-local gate shared by file operations and the explicit update action.
#[derive(Default)]
pub(crate) struct UpdateInstallGate(Mutex<State>);

/// Owns a tracked operation slot; releasing it never requires frontend cooperation.
pub(crate) struct OperationGuard<'a>(&'a UpdateInstallGate);

impl UpdateInstallGate {
    /// Starts an operation unless installation has reserved process shutdown.
    pub(crate) fn enter(&self) -> Result<OperationGuard<'_>, String> {
        let mut state = self.0.lock().map_err(|_| "更新安全状态不可用")?;
        if state.installing {
            return Err("正在安装更新，请等待应用重新启动。".into());
        }
        state.active += 1;
        Ok(OperationGuard(self))
    }

    /// Atomically reserves shutdown only when no tracked operation is active.
    pub(crate) fn reserve(&self) -> Result<(), String> {
        let mut state = self.0.lock().map_err(|_| "更新安全状态不可用")?;
        if state.installing || state.active != 0 {
            return Err("仍有操作正在执行，请等待清理、恢复或管理员维护完成后重试。".into());
        }
        state.installing = true;
        Ok(())
    }

    /// Re-enables operations if the installer could not be launched.
    pub(crate) fn release(&self) -> Result<(), String> {
        self.0.lock().map_err(|_| "更新安全状态不可用")?.installing = false;
        Ok(())
    }
}

impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.lock() {
            state.active = state.active.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateInstallGate;

    #[test]
    fn active_operations_block_installation_until_all_guards_drop() {
        let gate = UpdateInstallGate::default();
        let first = gate.enter().unwrap();
        let second = gate.enter().unwrap();
        assert!(gate.reserve().is_err());
        drop(first);
        assert!(gate.reserve().is_err());
        drop(second);
        assert!(gate.reserve().is_ok());
    }

    #[test]
    fn installation_excludes_new_operations_and_releases_after_failure() {
        let gate = UpdateInstallGate::default();
        gate.reserve().unwrap();
        assert!(gate.enter().is_err());
        assert!(gate.reserve().is_err());
        gate.release().unwrap();
        assert!(gate.enter().is_ok());
    }
}
