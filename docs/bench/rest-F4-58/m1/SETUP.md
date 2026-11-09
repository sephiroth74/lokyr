# A Mac of no management for F4-58's measures

F4-58 found `allow-jit` lengthening Lokyr's latency's tail in some sessions and not in others, on a Mac that Microsoft Defender and Jamf manage, whose activity the sessions could not control ([rest-F4-58](../../rest-F4-58.md)). These steps prepare a MacBook Pro M1 of no management, as most of Lokyr's users have, so that the same measures run there, driven over SSH from the Mac of the repository: the latency of 0.7.0's and 0.8.0's apps, each also signed again, 0.7.0's with `allow-jit` and 0.8.0's without; the startup; the entitlement's micro-benchmarks; and, if Xcode is there, the window's motion, which Defender's scans of Instruments' recordings kept from the managed Mac.

## 1. macOS

- macOS 26 or later, Lokyr's least, and its updates installed before the measures, so that none starts during them.
- Signed in to the account that the measures run in, on its desktop; the power adapter in, Low Power Mode off.
- After a reset, macOS indexes the disk for Spotlight and analyzes the photos for an hour or more: the measures wait until the Mac is at rest, which the session checks before it starts.

## 2. The screen awake and unlocked

The apps draw windows, which a locked or sleeping screen shows none of. In System Settings:

- **Lock Screen**: "Start Screen Saver when inactive" Never; "Turn display off on power adapter when inactive" Never; "Require password after screen saver begins or display is turned off" Never, for the measures' time.
- The lid open, the built-in display the only one, as the sessions measure at its refresh.

The session also holds `caffeinate -dimsu` while it runs.

## 3. Remote Login

- **General › Sharing › Remote Login** on, "Allow access for" the account of step 1 alone.
- The public key of the Mac of the repository, which the session gives, appended to that account's `~/.ssh/authorized_keys`:

  ```sh
  mkdir -p ~/.ssh && chmod 700 ~/.ssh
  echo '<the public key>' >> ~/.ssh/authorized_keys
  chmod 600 ~/.ssh/authorized_keys
  ```

- The Mac's name on the network, `scutil --get LocalHostName`, and the account's, `whoami`, told to the session, which connects as `<account>@<name>.local`.

## 4. For the window's motion, optional

- Xcode from the App Store, opened once to accept its license, as Instruments' `xctrace` comes with it alone.
- `lokyr-bench motion` moves the mouse of synthetic events, which macOS asks Accessibility's permission for at its first run: System Settings › Privacy & Security › Accessibility, the binary that the session names allowed.

## 5. During the measures

About 40 minutes, and 20 more for the motion: the Mac left alone, no key, no mouse, the session following it over SSH.

## 6. After

- The key's line taken out of `~/.ssh/authorized_keys`, and Remote Login off, if it is not wanted.
- The copies of the apps and the measures in `~/lokyr-bench`, which the session copied there, removed.
