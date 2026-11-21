//! The steps of the maintenance scenarios: an update of InayOS through `pacman -Syu` from a
//! test copy of its repository, the Arch news that a pacman upgrade shows, and snapper's
//! number cleanup after many transactions.

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::boot_checks;
use super::file_server::FileServer;
use super::maintenance_checks::{self as checks, MANUAL_LINK, MANUAL_TITLE, PLAIN_TITLE};
use super::test_repository::{self, INAYOS_PACKAGE, Package, TestRepository};
use super::{Scenario, StepError, sudo, user_power_off};
use crate::ssh::Guest;

/// The pkgrels of the two `inayos` builds. Both are above the pkgrel of a release or of the
/// USB stick's local repository, so the first build is an upgrade of the kept disk's package
/// and the second an upgrade of the first.
const FIRST_PKGREL: u32 = 1000;
const SECOND_PKGREL: u32 = 1001;
const PACMAN_CONF: &str = "/etc/pacman.conf";
/// The file that the inayos-repository module includes from pacman.conf, with the [inayos]
/// section and its Server.
const REPOSITORY_CONF: &str = "/etc/pacman.d/inayos.conf";
const INCLUDE_LINE: &str = "Include = /etc/pacman.d/inayos.conf";
/// The maintenance module's configuration of inayos-news, whose `url` names the feed.
const NEWS_CONF: &str = "/etc/inayos-news.conf";
const NEWS_FILE: &str = "news.xml";
const GUARD_STATUS: &str = "/var/lib/inayos-guard/status.json";
/// The guard's pacman hook, which the `inayos` package installs.
const GUARD_HOOK: &str = "/usr/share/libalpm/hooks/zzz-inayos-guard.hook";
/// The packaged guard, a path that the guard's hook must watch so that an update of InayOS
/// checks the boot files with the new guard.
const GUARD_TARGET: &str = "usr/bin/inayos-guard";
const SNAPPER_CONFIG: &str = "/etc/snapper/configs/root";
/// snapper's number cleanup leaves snapshots younger than `NUMBER_MIN_AGE`, an hour in the
/// snapshots module's configuration. The guest's clock moves past it before the cleanup.
const CLOCK_JUMP: &str = "+2 hours";

impl Scenario {
    /// Updates InayOS from a test repository with `pacman -Syu`, and checks the new package,
    /// the guard's hook and the next boot.
    pub(super) fn check_pacman_update(&mut self) -> Result<(), StepError> {
        let user = self.boot_installed()?;
        let repository = self.test_repository()?;
        let first = self.build_inayos(FIRST_PKGREL)?;
        repository.add(&first)?;
        let server = self.serve(repository.folder())?;
        self.use_test_repository(&user, &repository, &server)?;

        println!(
            "vm: installing {INAYOS_PACKAGE} {} from the test repository",
            first.version
        );
        user.run_logged(
            &sudo(&format!("pacman -Sy --noconfirm {INAYOS_PACKAGE}")),
            &self.log("pacman-first"),
        )?;
        self.check_installed(&user, &first)?;
        // Without this trigger, the hook runs during the update only when the Arch mirrors
        // also bring a kernel, firmware or another watched file that day.
        let hook = user.run(&format!("cat {GUARD_HOOK}"))?;
        if !checks::hook_has_path_target(&hook, GUARD_TARGET) {
            return Err(StepError::Check(format!(
                "{GUARD_HOOK} has no path trigger on {GUARD_TARGET}, so an update of {INAYOS_PACKAGE} does not run the guard"
            )));
        }

        let second = self.build_inayos(SECOND_PKGREL)?;
        repository.add(&second)?;
        let guard_before = user.run(&sudo(&format!("stat -c %Y {GUARD_STATUS}")))?;
        println!("vm: updating the system with pacman -Syu");
        // A kept disk can be older than keys that signed newer Arch packages.
        user.run_logged(
            &sudo("pacman -Sy --noconfirm --needed archlinux-keyring"),
            &self.log("pacman-keyring"),
        )?;
        user.run_logged(&sudo("pacman -Syu --noconfirm"), &self.log("pacman-syu"))?;
        self.check_installed(&user, &second)?;
        let guard_after = user.run(&sudo(&format!("stat -c %Y {GUARD_STATUS}")))?;
        if guard_after.trim() == guard_before.trim() {
            return Err(StepError::Check(format!(
                "the guard's hook did not run during the update: {GUARD_STATUS} still has the time {}",
                guard_before.trim()
            )));
        }
        println!("vm: the guard's hook ran during the update");

        self.power_off(&user, &user_power_off())?;
        let user = self.boot_installed()?;
        self.check_installed(&user, &second)?;
        self.check_guard_boot_ran(&user)?;
        println!("vm: the updated system boots");
        drop(server);
        Ok(())
    }

    /// Serves a news feed with an unread manual-step item, and checks that a pacman upgrade
    /// shows it until `inayos-news` marks it read.
    pub(super) fn check_arch_news(&mut self) -> Result<(), StepError> {
        let user = self.boot_installed()?;
        let repository = self.test_repository()?;
        let package = self.build_inayos(FIRST_PKGREL)?;
        repository.add(&package)?;
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs())
            .unwrap_or_default();
        let feed = repository.folder().join(NEWS_FILE);
        fs::write(&feed, checks::news_feed(seconds))
            .map_err(|error| StepError::Io(feed.clone(), error))?;
        let server = self.serve(repository.folder())?;
        self.use_test_repository(&user, &repository, &server)?;

        let conf = self.write(
            "inayos-news.conf",
            &format!("url={}/{NEWS_FILE}\n", server.guest_url()),
        )?;
        user.copy_to(&conf, "/tmp/inayos-news.conf")?;
        user.run(&sudo(&format!(
            "install -m 644 /tmp/inayos-news.conf {NEWS_CONF}"
        )))?;
        println!(
            "vm: installing {INAYOS_PACKAGE} {} from the test repository",
            package.version
        );
        user.run_logged(
            &sudo(&format!("pacman -Sy --noconfirm {INAYOS_PACKAGE}")),
            &self.log("pacman-install"),
        )?;
        self.check_installed(&user, &package)?;

        println!("vm: upgrading with an unread manual-step item in the news");
        let unread = self.pacman_upgrade(&user, "pacman-unread")?;
        if !unread.contains(MANUAL_TITLE) || !unread.contains(MANUAL_LINK) {
            return Err(StepError::Check(format!(
                "the upgrade did not show the unread item {MANUAL_TITLE:?} with its link"
            )));
        }
        if unread.contains(PLAIN_TITLE) {
            return Err(StepError::Check(format!(
                "the upgrade showed {PLAIN_TITLE:?}, which announces no manual step"
            )));
        }
        user.run_logged(&sudo("inayos-news"), &self.log("inayos-news"))?;
        let shown = self.read_log("inayos-news")?;
        if !shown.contains(MANUAL_TITLE) {
            return Err(StepError::Check(format!(
                "inayos-news did not show the unread item {MANUAL_TITLE:?}"
            )));
        }

        println!("vm: upgrading again after inayos-news marked the news read");
        let read = self.pacman_upgrade(&user, "pacman-read")?;
        if read.contains(MANUAL_TITLE) || read.contains(PLAIN_TITLE) {
            return Err(StepError::Check(
                "the upgrade after inayos-news still showed news items".to_owned(),
            ));
        }
        println!("vm: the upgrade showed the manual step once, and nothing after it was read");
        drop(server);
        Ok(())
    }

    /// Runs more pacman transactions than snapper keeps, runs the number cleanup, and checks
    /// that it kept its limits.
    pub(super) fn check_snapshot_cleanup(&mut self) -> Result<(), StepError> {
        let user = self.boot_installed()?;
        let marker = self.marker_in_guest(&user)?;
        let config = user.run(&format!("cat {SNAPPER_CONFIG}"))?;
        self.write("snapper-config-root", &config)?;
        let limits = checks::snapper_limits(&config).map_err(StepError::Check)?;
        // Each transaction adds a pre and a post snapshot, and the list must pass both limits.
        let transactions = (limits.number + limits.important) / 2 + 2;
        println!(
            "vm: running {transactions} pacman transactions; snapper keeps {} snapshots and {} important",
            limits.number, limits.important
        );
        for transaction in 1..=transactions {
            user.run_logged(
                &sudo(&format!("pacman -U --noconfirm {marker}")),
                &self.log(&format!("pacman-transaction-{transaction}")),
            )?;
        }
        let before = self.snapper_list(&user, "before-cleanup")?;
        let count_before = checks::number_snapshots(&before).len();
        if count_before <= limits.number {
            return Err(StepError::Check(format!(
                "snapper lists {count_before} number snapshots after the transactions, not more than its limit of {}",
                limits.number
            )));
        }
        let newest = checks::number_snapshots(&before)
            .iter()
            .map(|snapshot| snapshot.number)
            .max()
            .unwrap_or_default();

        println!("vm: moving the guest's clock past snapper's minimum age and running the cleanup");
        user.run(&sudo("timedatectl set-ntp false"))?;
        user.run(&sudo(&format!("date -s '{CLOCK_JUMP}'")))?;
        user.run_logged(
            &sudo("systemctl start snapper-cleanup.service"),
            &self.log("snapper-cleanup"),
        )?;
        let after = self.snapper_list(&user, "after-cleanup")?;
        if let Some(problem) = checks::cleanup_problem(&after, limits) {
            return Err(StepError::Check(problem));
        }
        let count_after = checks::number_snapshots(&after).len();
        if count_after >= count_before {
            return Err(StepError::Check(format!(
                "the cleanup deleted nothing: {count_before} number snapshots before and {count_after} after"
            )));
        }
        if !after.iter().any(|snapshot| snapshot.number == newest) {
            return Err(StepError::Check(format!(
                "the cleanup deleted the newest snapshot {newest}"
            )));
        }
        println!(
            "vm: the cleanup left {count_after} of {count_before} number snapshots, within its limits"
        );
        Ok(())
    }

    /// A new test repository in the layer.
    fn test_repository(&self) -> Result<TestRepository, StepError> {
        Ok(TestRepository::create(
            self.layer.dir(),
            &self.layer.dir().join("logs"),
        )?)
    }

    /// Packages the binaries that the xtask built as `inayos` with `pkgrel`, the same way as
    /// the stick's repository, each pkgrel in its own folder of the layer.
    fn build_inayos(&self, pkgrel: u32) -> Result<Package, StepError> {
        println!("vm: building the {INAYOS_PACKAGE} package with pkgrel {pkgrel}");
        let built = crate::packages::build_repository(
            &self.repository,
            &self.binaries,
            &self.layer.dir().join(format!("{INAYOS_PACKAGE}-{pkgrel}")),
            pkgrel,
        )?;
        Ok(test_repository::find_package(
            &built.folder,
            INAYOS_PACKAGE,
        )?)
    }

    /// Builds the marker package and copies it into the guest, returning its path there.
    pub(super) fn marker_in_guest(&self, user: &Guest) -> Result<String, StepError> {
        let package = test_repository::build_marker(
            &self.layer.dir().join("test-packages"),
            &self.layer.dir().join("logs"),
        )?;
        let name = package
            .file
            .file_name()
            .expect("a package file has a name")
            .to_string_lossy()
            .into_owned();
        let remote = format!("/tmp/{name}");
        user.copy_to(&package.file, &remote)?;
        Ok(remote)
    }

    fn serve(&self, folder: &std::path::Path) -> Result<FileServer, StepError> {
        let server =
            FileServer::start(folder).map_err(|error| StepError::Io(folder.to_owned(), error))?;
        println!(
            "vm: serving {} to the guest at {}",
            folder.display(),
            server.guest_url()
        );
        Ok(server)
    }

    /// Makes the guest's pacman trust the test repository's key, with a local signature, and
    /// points the [inayos] section at the file server.
    fn use_test_repository(
        &self,
        user: &Guest,
        repository: &TestRepository,
        server: &FileServer,
    ) -> Result<(), StepError> {
        if user
            .run(&format!("grep -qxF '{INCLUDE_LINE}' {PACMAN_CONF}"))
            .is_err()
        {
            return Err(StepError::Check(format!(
                "{PACMAN_CONF} of the kept disk has no line {INCLUDE_LINE:?}; run the blank-disk scenario again: cargo xtask vm scenario blank-disk"
            )));
        }
        user.copy_to(repository.public_key(), "/tmp/inayos-test-repository.asc")?;
        user.run_logged(
            &format!(
                "{} && {}",
                sudo("pacman-key --add /tmp/inayos-test-repository.asc"),
                sudo(&format!(
                    "pacman-key --lsign-key {}",
                    repository.fingerprint()
                ))
            ),
            &self.log("pacman-key"),
        )?;
        let conf = self.write(
            "inayos.conf",
            &format!("[inayos]\nServer = {}\n", server.guest_url()),
        )?;
        user.copy_to(&conf, "/tmp/inayos.conf")?;
        user.run(&sudo(&format!(
            "install -m 644 /tmp/inayos.conf {REPOSITORY_CONF}"
        )))?;
        Ok(())
    }

    /// Checks that pacman lists `package` with its version as installed.
    fn check_installed(&self, user: &Guest, package: &Package) -> Result<(), StepError> {
        let listed = user.run(&format!("pacman -Q {INAYOS_PACKAGE} || true"))?;
        let version = checks::installed_version(&listed, INAYOS_PACKAGE);
        if version != Some(package.version.as_str()) {
            return Err(StepError::Check(format!(
                "pacman lists {INAYOS_PACKAGE} as {version:?}, not {}",
                package.version
            )));
        }
        println!("vm: {INAYOS_PACKAGE} {} is installed", package.version);
        Ok(())
    }

    /// Reinstalls `inayos` from the test repository, an upgrade transaction that runs the news
    /// hook, and returns pacman's output.
    fn pacman_upgrade(&self, user: &Guest, label: &str) -> Result<String, StepError> {
        user.run_logged(
            &sudo(&format!("pacman -S --noconfirm {INAYOS_PACKAGE}")),
            &self.log(label),
        )?;
        self.read_log(label)
    }

    fn read_log(&self, label: &str) -> Result<String, StepError> {
        let path = self.log(label);
        fs::read(&path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .map_err(|error| StepError::Io(path, error))
    }

    /// Reads snapper's list of the root configuration and keeps a copy named after `label`.
    fn snapper_list(
        &self,
        user: &Guest,
        label: &str,
    ) -> Result<Vec<boot_checks::Snapshot>, StepError> {
        let json = user.run(&sudo("snapper --no-dbus --jsonout -c root list"))?;
        self.write(&format!("snapper-{label}.json"), &json)?;
        boot_checks::snapshots(&json).map_err(StepError::Check)
    }
}
