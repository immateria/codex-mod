pub(crate) fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "android")]
    {
        use std::io;
        use std::process::{Command, Stdio};

        Command::new("termux-open-url")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|err| {
                if err.kind() == io::ErrorKind::NotFound {
                    anyhow::anyhow!(
                        "`termux-open-url` not found (install `termux-tools` or copy/paste the URL): {err}"
                    )
                } else {
                    anyhow::anyhow!("failed to run `termux-open-url`: {err}")
                }
            })?;
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        use anyhow::Context;

        webbrowser::open(url).context("webbrowser::open failed")?;
        Ok(())
    }
}
