package org.unluminous.completioneval;

import com.intellij.openapi.project.Project;
import org.rust.cargo.project.model.CargoProject;
import org.rust.cargo.project.model.CargoProjectsService;
import org.rust.cargo.project.settings.RustProjectSettingsService;
import org.rust.cargo.project.settings.RustProjectSettingsServiceKt;

import java.nio.file.Path;
import java.util.Collection;
import java.util.StringJoiner;

/** Waits for the Rust plugin to import a Cargo project. Loaded only when the Rust plugin is present. */
final class RustReady {
    private RustReady() {
    }

    /**
     * Blocks until the Cargo workspace, the standard library and rustc info are loaded. The project
     * opens through the Rust plugin's own open processor, which attaches the Cargo project itself.
     * @param project - the open project
     * @param root - corpus root holding Cargo.toml
     * @param timeoutSeconds - longest time to wait
     */
    static String await(Project project, Path root, int timeoutSeconds) throws Exception {
        CargoProjectsService service = project.getService(CargoProjectsService.class);
        RustProjectSettingsService settings = RustProjectSettingsServiceKt.getRustSettings(project);
        System.out.println("[completionEval] rust toolchain before: " + settings.getToolchain());
        if (settings.getToolchain() == null) {
            String home = cargoHomeOnPath();
            System.out.println("[completionEval] rust toolchain home from PATH: " + home);
            settings.modify(state -> {
                state.setToolchainHomeDirectory(home);
                return kotlin.Unit.INSTANCE;
            });
            System.out.println("[completionEval] rust toolchain now: " + settings.getToolchain());
        }
        long start = System.currentTimeMillis();
        long deadline = start + timeoutSeconds * 1000L;
        boolean discovered = false;
        while (System.currentTimeMillis() < deadline) {
            if (service.getAllProjects().isEmpty() && !discovered && System.currentTimeMillis() - start > 5_000) {
                service.attachCargoProjects(java.util.List.of(root.resolve("Cargo.toml")));
                discovered = true;
            }
            if (service.getInitialized() && !service.isRefreshInProgress() && statuses(service).startsWith("ok")) break;
            if ((System.currentTimeMillis() - start) % 10_000 < 600) System.out.println("[completionEval] cargo wait: init=" + service.getInitialized() + " refreshing=" + service.isRefreshInProgress() + " " + statuses(service));
            Thread.sleep(500);
        }
        return statuses(service);
    }

    /**
     * Finds the folder holding cargo on PATH, the way the Rust plugin would suggest a toolchain home.
     */
    private static String cargoHomeOnPath() {
        for (String dir : System.getenv("PATH").split(java.io.File.pathSeparator)) {
            if (new java.io.File(dir, "cargo.exe").isFile() || new java.io.File(dir, "cargo").isFile()) return dir;
        }
        throw new IllegalStateException("cargo not found on PATH");
    }

    /** Reduces a status object to its class name. */
    private static String brief(Object status) {
        return String.valueOf(status).replaceAll("^.*[$]", "").replaceAll("@.*$", "");
    }

    /**
     * Summarises the update status of every attached Cargo project.
     * @param service - the Cargo projects service
     */
    private static String statuses(CargoProjectsService service) {
        Collection<CargoProject> all = service.getAllProjects();
        boolean ok = !all.isEmpty();
        StringJoiner joiner = new StringJoiner("; ");
        for (CargoProject p : all) {
            String merged = String.valueOf(p.getMergedStatus()).replace("org.rust.cargo.project.model.CargoProject$UpdateStatus$", "");
            // A failed build script evaluation (dependencies that could not be built) does not stop completion from working.
            boolean loaded = String.valueOf(p.getWorkspaceStatus()).contains("UpToDate") && String.valueOf(p.getStdlibStatus()).contains("UpToDate") && String.valueOf(p.getRustcInfoStatus()).contains("UpToDate");
            if (!loaded || String.valueOf(p.getBuildScriptEvaluationStatus()).contains("NeedsUpdate")) ok = false;
            joiner.add(p.getPresentableName() + " merged=" + merged + " ws=" + brief(p.getWorkspaceStatus()) + " stdlib=" + brief(p.getStdlibStatus()) + " rustc=" + brief(p.getRustcInfoStatus()) + " bs=" + brief(p.getBuildScriptEvaluationStatus()));
        }
        return (ok ? "ok " : "pending ") + joiner;
    }
}
