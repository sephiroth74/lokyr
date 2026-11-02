//! `cargo xtask notarize`: has Apple notarize `target/Lokyr.app`, which
//! `cargo xtask bundle` signed with a Developer ID and the
//! hardened runtime, and staples the ticket to it, so that Gatekeeper opens
//! a downloaded copy without its warning.
//!
//! The bundle goes to Apple's notary service as a zip, through `notarytool`,
//! with the credentials of the keychain's profile `lokyr-notary`, or of the
//! one that `LOKYR_NOTARY_PROFILE` names, in the environment or in the
//! untracked `private/lokyr.env`; the key stays in the keychain,
//! outside the repository. Apple's log tells why a bundle that it refuses
//! was refused.

use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::bundle;

/// The keychain's profile of the notary's credentials, by default.
const PROFILE: &str = "lokyr-notary";

/// The variable that names another profile.
const PROFILE_VARIABLE: &str = "LOKYR_NOTARY_PROFILE";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    if let Some(arg) = args.next() {
        bail!("notarize takes no arguments, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let app = root.join("target").join(format!("{}.app", bundle::NAME));
    if !app.exists() {
        bail!("{} is not there: `just bundle` builds it", app.display());
    }
    ensure_developer_id(&app)?;
    notarize_app(&app)?;
    println!(
        "notarize: {} is notarized, its ticket stapled",
        app.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// Fails unless `app` is signed with a Developer ID, which notarization
/// needs.
pub(crate) fn ensure_developer_id(app: &Path) -> Result<()> {
    let signature = output(
        Command::new("codesign")
            .args(["--display", "--verbose=2"])
            .arg(app),
        "reading the bundle's signature",
    )?;
    if !developer_id_signed(&signature) {
        bail!(
            "{} is not signed with a Developer ID, which notarization needs: `just bundle` signs it so where `LOKYR_SIGN_IDENTITY` names one",
            app.display()
        );
    }
    Ok(())
}

/// Has Apple notarize the bundle `app`, signed with a Developer ID, as a
/// zip, staples the ticket to it, and assesses it as Gatekeeper does.
pub(crate) fn notarize_app(app: &Path) -> Result<()> {
    let zip = app.with_extension("zip");
    let _ = std::fs::remove_file(&zip);
    status(
        Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(app)
            .arg(&zip),
        "zipping the bundle",
    )?;
    let submitted = submit(&zip);
    let _ = std::fs::remove_file(&zip);
    submitted?;
    staple(app)?;
    status(
        Command::new("spctl")
            .args(["--assess", "--type", "execute", "--verbose=2"])
            .arg(app),
        "assessing the bundle as Gatekeeper does",
    )
}

/// Sends `file`, a zip, a disk image or an installer's package, to Apple's
/// notary service, with the credentials of the keychain's profile, and
/// waits for its answer; fails, with Apple's log of why, if Apple refuses
/// it.
pub(crate) fn submit(file: &Path) -> Result<()> {
    let profile = crate::private::variable(PROFILE_VARIABLE)?
        .filter(|named| !named.is_empty())
        .unwrap_or_else(|| PROFILE.to_owned());
    println!(
        "notarize: sending {} to Apple's notary service",
        file.display()
    );
    let submitted = output(
        Command::new("xcrun")
            .args(["notarytool", "submit"])
            .arg(file)
            .args([
                "--keychain-profile",
                &profile,
                "--wait",
                "--output-format",
                "json",
            ]),
        "sending it to the notary service",
    )?;
    let (id, outcome) = submission(&submitted)
        .with_context(|| format!("the notary service's answer: {submitted}"))?;
    println!("notarize: submission {id}: {outcome}");
    if outcome != "Accepted" {
        let _ = Command::new("xcrun")
            .args(["notarytool", "log", &id, "--keychain-profile", &profile])
            .status();
        bail!("Apple did not notarize {}: {outcome}", file.display());
    }
    Ok(())
}

/// Staples the ticket of Apple's notarization to `path`, a bundle or a disk
/// image that Apple notarized, and checks it.
pub(crate) fn staple(path: &Path) -> Result<()> {
    status(
        Command::new("xcrun").args(["stapler", "staple"]).arg(path),
        "stapling the ticket",
    )?;
    status(
        Command::new("xcrun")
            .args(["stapler", "validate"])
            .arg(path),
        "checking the stapled ticket",
    )
}

/// Whether `signature`, as `codesign --display --verbose=2` tells it, is of
/// a Developer ID Application, and not ad hoc.
pub(crate) fn developer_id_signed(signature: &str) -> bool {
    signature
        .lines()
        .any(|line| line.starts_with("Authority=Developer ID Application:"))
}

/// The submission's id and its status, of `notarytool`'s answer in JSON.
fn submission(answer: &str) -> Option<(String, String)> {
    let field = |name: &str| {
        Regex::new(&format!(r#""{name}"\s*:\s*"([^"]*)""#))
            .ok()?
            .captures(answer)
            .map(|found| found[1].to_owned())
    };
    Some((field("id")?, field("status")?))
}

/// Runs `command`, which `what` describes, and fails if it fails.
pub(crate) fn status(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    if !status.success() {
        bail!("{what} failed: {status}");
    }
    Ok(())
}

/// Runs `command`, which `what` describes, and gives what it wrote, its
/// output and its errors, which `codesign` writes its display to; fails if
/// it fails.
pub(crate) fn output(command: &mut Command, what: &str) -> Result<String> {
    let output = command.output().with_context(|| what.to_owned())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!("{what} failed: {}: {text}", output.status);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_of_a_developer_id_is_told_from_an_ad_hoc_one() {
        let signed = "Executable=/x/Lokyr.app/Contents/MacOS/lokyr\n\
            Authority=Developer ID Application: The Maintainer (ABCDE12345)\n\
            Authority=Developer ID Certification Authority\n";
        assert!(developer_id_signed(signed));
        let ad_hoc = "Executable=/x/Lokyr.app/Contents/MacOS/lokyr\nSignature=adhoc\n";
        assert!(!developer_id_signed(ad_hoc));
    }

    #[test]
    fn the_notary_s_answer_gives_its_id_and_status() {
        let answer = r#"{"message":"Processing complete","id":"the submission's id","status":"Accepted"}"#;
        assert_eq!(
            submission(answer),
            Some((
                "the submission's id".to_owned(),
                "Accepted".to_owned()
            ))
        );
        assert_eq!(submission(r#"{"status":"Invalid"}"#), None);
    }
}
