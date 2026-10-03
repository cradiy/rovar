use rovar_api::SessionDevice;

pub(crate) fn session_device() -> SessionDevice {
    #[cfg(not(target_family = "wasm"))]
    let device = SessionDevice {
        system: whoami::distro().unwrap_or_else(|_| whoami::platform().to_string()),
        name: whoami::devicename()
            .or_else(|_| whoami::hostname())
            .unwrap_or_default(),
        client: "Rovar Desktop".into(),
    };
    #[cfg(target_family = "wasm")]
    let device = browser_device(
        &web_sys::window()
            .and_then(|window| window.navigator().user_agent().ok())
            .unwrap_or_default(),
    );
    SessionDevice {
        system: clean(&device.system),
        name: clean(&device.name),
        client: clean(&device.client),
    }
}

fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect()
}

#[cfg(any(target_family = "wasm", test))]
fn browser_device(agent: &str) -> SessionDevice {
    let (system, name) = if agent.contains("iPad") {
        ("iPadOS", "iPad")
    } else if agent.contains("iPhone") || agent.contains("iPod") {
        ("iOS", "iPhone")
    } else if agent.contains("Android") {
        (
            "Android",
            if agent.contains("Mobile") {
                "Android phone"
            } else {
                "Android tablet"
            },
        )
    } else if agent.contains("Windows") {
        ("Windows", "PC")
    } else if agent.contains("CrOS") {
        ("ChromeOS", "Chromebook")
    } else if agent.contains("Macintosh") || agent.contains("Mac OS X") {
        ("macOS", "Mac")
    } else if agent.contains("Linux") {
        ("Linux", "PC")
    } else {
        ("", "")
    };
    let client = if agent.contains("Edg/") || agent.contains("EdgiOS/") || agent.contains("EdgA/") {
        "Edge"
    } else if agent.contains("OPR/") || agent.contains("OPiOS/") {
        "Opera"
    } else if agent.contains("Firefox/") || agent.contains("FxiOS/") {
        "Firefox"
    } else if agent.contains("Chrome/") || agent.contains("CriOS/") {
        "Chrome"
    } else if agent.contains("Safari/") {
        "Safari"
    } else {
        "Web browser"
    };
    SessionDevice {
        system: system.into(),
        name: name.into(),
        client: client.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_metadata_distinguishes_overlapping_user_agents() {
        for (agent, system, name, client) in [
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/140 Safari/537 Edg/140",
                "Windows",
                "PC",
                "Edge",
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) CriOS/140 Mobile Safari/604",
                "iOS",
                "iPhone",
                "Chrome",
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64) Firefox/140",
                "Linux",
                "PC",
                "Firefox",
            ),
            (
                "Mozilla/5.0 (Linux; Android 15) Chrome/140 Mobile Safari/537",
                "Android",
                "Android phone",
                "Chrome",
            ),
            ("unknown", "", "", "Web browser"),
        ] {
            let device = browser_device(agent);
            assert_eq!(
                (
                    device.system.as_str(),
                    device.name.as_str(),
                    device.client.as_str()
                ),
                (system, name, client)
            );
        }
    }
}
