//! ライブラリを利用するスレッドの初期化。

use std::marker::PhantomData;

use crate::error::Result;

/// オーディオライブラリを利用するスレッドの初期化状態を保持するガード。
///
/// [`initialize_thread`] が返す。スレッドの終了まで保持すること。
/// 初期化したスレッド以外で drop されないよう `!Send` / `!Sync` にしている。
pub struct ThreadGuard {
    /// Windows では COM の MTA 初期化を保持する。
    #[cfg(enable_wasapi)]
    _com_guard: crate::device_wasapi::ComGuard,
    /// このガードを `!Send` / `!Sync` にするためのマーカー。値は保持しない。
    _not_send: PhantomData<*const ()>,
}

/// 現在のスレッドをオーディオライブラリを利用できる状態に初期化する。
///
/// ライブラリを使うスレッドごとに、そのスレッドの先頭で呼ぶこと。
/// 返された [`ThreadGuard`] はスレッドの終了まで保持すること。
/// Windows では COM を MTA モードで初期化する。macOS / Linux では何もしない。
///
/// # Errors
///
/// Windows で COM を MTA モードで初期化できなかった場合にエラーを返す。
/// 呼び出し元スレッドが STA で初期化済みの場合などに発生する。
pub fn initialize_thread() -> Result<ThreadGuard> {
    #[cfg(enable_wasapi)]
    {
        Ok(ThreadGuard {
            _com_guard: crate::device_wasapi::ComGuard::init_mta()?,
            _not_send: PhantomData,
        })
    }
    #[cfg(not(enable_wasapi))]
    {
        Ok(ThreadGuard {
            _not_send: PhantomData,
        })
    }
}
