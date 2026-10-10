package org.unluminous.completioneval;

import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.intellij.codeInsight.CodeInsightSettings;
import com.intellij.ide.impl.OpenProjectTask;
import com.intellij.ide.impl.ProjectUtil;
import com.intellij.openapi.application.ApplicationStarter;
import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.fileEditor.FileEditorManager;
import com.intellij.openapi.fileEditor.OpenFileDescriptor;
import com.intellij.openapi.project.DumbService;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.project.ex.ProjectManagerEx;
import com.intellij.openapi.vfs.LocalFileSystem;
import com.intellij.openapi.vfs.VirtualFile;

import java.io.BufferedWriter;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Headless entry point: opens one corpus, waits for indexing and project import, runs every
 * query of that corpus and writes results.jsonl lines. Started as idea64 completionEval job.json.
 */
public final class CompletionEvalStarter implements ApplicationStarter {
    private static final Gson GSON = new Gson();
    private Path logFile;

    @Override
    public int getRequiredModality() {
        return ApplicationStarter.NOT_IN_EDT;
    }

    @Override
    public boolean isHeadless() {
        return true;
    }

    /**
     * Reads the job file named by the first argument and runs it, always ending the process at the end.
     * @param args - command name followed by the job file path
     */
    @Override
    public void main(List<String> args) {
        int code = 0;
        try {
            runJob(Paths.get(args.get(1)));
        } catch (Throwable t) {
            log("FATAL " + stack(t));
            code = 1;
        }
        log("exiting with " + code);
        System.out.flush();
        Runtime.getRuntime().halt(code);
    }

    /**
     * Runs one corpus: open, wait, query, close, and write the import report.
     * @param jobFile - JSON job description written by run.ps1
     */
    private void runJob(Path jobFile) throws Exception {
        JsonObject job = JsonParser.parseString(Files.readString(jobFile, StandardCharsets.UTF_8)).getAsJsonObject();
        CodeInsightSettings.getInstance().AUTOCOMPLETE_ON_CODE_COMPLETION = false;
        Path out = Paths.get(job.get("outDir").getAsString());
        String corpus = job.get("corpus").getAsString();
        logFile = out.resolve("intellij-" + corpus + ".log");
        Path corpusDir = Paths.get(job.get("corpusDir").getAsString());
        String language = job.get("language").getAsString();
        Map<String, Object> report = new LinkedHashMap<>();
        report.put("corpus", corpus);
        report.put("started", java.time.Instant.now().toString());
        report.put("ml", applyMl(job.get("ml").getAsBoolean()));
        List<Query> queries = Positions.load(Paths.get(job.get("positions").getAsString()), corpus, job.get("split").getAsString(), job.get("limit").getAsInt(), corpusDir);
        log("queries: " + queries.size());
        long t0 = System.currentTimeMillis();
        Project project = ProjectUtil.openOrImport(corpusDir, OpenProjectTask.Companion.build());
        if (project == null) throw new IllegalStateException("project did not open");
        report.put("openMs", System.currentTimeMillis() - t0);
        waitUntilReady(project, corpusDir, language, report);
        report.put("importMs", System.currentTimeMillis() - t0);
        log("ready after " + report.get("importMs") + " ms");
        runQueries(project, corpusDir, queries, out.resolve("results.jsonl"), report, language);
        report.put("ended", java.time.Instant.now().toString());
        Files.writeString(out.resolve("import-" + corpus + ".json"), GSON.toJson(report), StandardCharsets.UTF_8);
        QueryRunner.onEdt(() -> ProjectManagerEx.getInstanceEx().forceCloseProject(project));
    }

    /**
     * Applies the ML ranking switch if the ranking plugin is installed.
     * @param enabled - true to keep ranking on
     */
    private String applyMl(boolean enabled) {
        try {
            String state = MlConfig.apply(enabled);
            log("ml " + (enabled ? "on" : "off") + ": " + state);
            return state;
        } catch (Throwable t) {
            log("ml settings unavailable: " + t);
            return "unavailable: " + t;
        }
    }

    /**
     * Waits for indexing, and for the Cargo import when the corpus is Rust.
     * @param project - the open project
     * @param corpusDir - corpus root
     * @param language - rust or typescript
     * @param report - receives what was waited for
     */
    private void waitUntilReady(Project project, Path corpusDir, String language, Map<String, Object> report) throws Exception {
        DumbService.getInstance(project).waitForSmartMode();
        log("smart mode");
        if (language.equals("rust")) {
            String status = RustReady.await(project, corpusDir, 900);
            log("cargo: " + status);
            report.put("cargo", status);
            DumbService.getInstance(project).waitForSmartMode();
        }
        Thread.sleep(2000);
        DumbService.getInstance(project).waitForSmartMode();
    }

    /**
     * Runs the queries file by file and appends one line each to results.jsonl.
     * @param project - the open project
     * @param corpusDir - corpus root
     * @param queries - all queries of the corpus
     * @param results - results.jsonl path
     * @param report - receives counts
     * @param language - rust or typescript
     */
    private void runQueries(Project project, Path corpusDir, List<Query> queries, Path results, Map<String, Object> report, String language) throws Exception {
        QueryRunner runner = new QueryRunner(project);
        if (language.equals("typescript")) warmUpTypeScript(project, corpusDir, queries, runner, report);
        int done = 0;
        int errors = 0;
        try (BufferedWriter writer = Files.newBufferedWriter(results, StandardCharsets.UTF_8, StandardOpenOption.CREATE, StandardOpenOption.APPEND)) {
            for (Map.Entry<String, List<Query>> file : Positions.byFile(queries).entrySet()) {
                Editor editor = openEditor(project, corpusDir.resolve(file.getKey()));
                if (language.equals("typescript")) warmFile(runner, editor, file.getValue(), 3);
                for (Query query : file.getValue()) {
                    QueryRunner.Result result = runner.run(editor, query);
                    writer.write(line(query, result));
                    writer.newLine();
                    writer.flush();
                    done++;
                    if (result.error != null) errors++;
                    if (done % 50 == 0) log("done " + done + "/" + queries.size());
                }
                closeEditor(project, corpusDir.resolve(file.getKey()));
            }
        }
        report.put("queries", done);
        report.put("errors", errors);
    }

    /**
     * Starts the TypeScript language service before the first measured query. tsserver is launched by the
     * first completion request and answers nothing until its project has loaded, so the best query of the
     * first file is repeated, and its answer discarded, until the service returns items.
     * @param project - the open project
     * @param corpusDir - corpus root
     * @param queries - all queries of the corpus
     * @param runner - the query runner
     * @param report - receives the warm up time
     */
    private void warmUpTypeScript(Project project, Path corpusDir, List<Query> queries, QueryRunner runner, Map<String, Object> report) throws Exception {
        Query probe = serviceProbe(queries, null);
        if (probe == null) return;
        long started = System.currentTimeMillis();
        Editor editor = openEditor(project, corpusDir.resolve(probe.path));
        boolean ready = warmFile(runner, editor, queriesOfFile(queries, probe.path), 120);
        closeEditor(project, corpusDir.resolve(probe.path));
        report.put("warmUpMs", System.currentTimeMillis() - started);
        report.put("warmUpReady", ready);
    }

    /**
     * Picks a query whose right answer only the TypeScript language service gives: a member or path
     * position with the longest prefix, so the expected name must be in the list once the service is up.
     * @param queries - queries to choose from
     * @param path - restrict to this file, or null for any file
     */
    private static Query serviceProbe(List<Query> queries, String path) {
        Query best = null;
        for (Query q : queries) {
            boolean classOk = q.positionClass.equals("member") || q.positionClass.equals("path");
            if (!classOk || q.expected.length() < 3 || (path != null && !q.path.equals(path))) continue;
            if (best == null || q.prefixLength > best.prefixLength) best = q;
        }
        return best;
    }

    /**
     * Returns the queries that belong to one file.
     * @param queries - all queries
     * @param path - file path
     */
    private static List<Query> queriesOfFile(List<Query> queries, String path) {
        List<Query> out = new java.util.ArrayList<>();
        for (Query q : queries) if (q.path.equals(path)) out.add(q);
        return out;
    }

    /**
     * Repeats the longest prefix query of a file, discarding the answer, until the language service
     * answers quickly with items, so the first measured query of the file is not a cold start.
     * @param runner - the query runner
     * @param editor - editor on the file
     * @param fileQueries - queries of that file
     * @param maxTries - most attempts
     * @return true when the service answered the probe, or when the file has no probe
     */
    private boolean warmFile(QueryRunner runner, Editor editor, List<Query> fileQueries, int maxTries) throws Exception {
        Query probe = serviceProbe(fileQueries, null);
        for (int tries = 1; probe != null && tries <= maxTries; tries++) {
            QueryRunner.Result result = runner.run(editor, probe);
            boolean answered = result.labels.contains(probe.expected);
            log("typescript warm up try " + tries + ": " + result.labels.size() + " labels, " + result.ms + " ms, expected found=" + answered);
            if (answered) return true;
            Thread.sleep(1000);
        }
        return probe == null;
    }

    /**
     * Formats one results.jsonl line.
     * @param query - the query
     * @param result - what it produced
     */
    private String line(Query query, QueryRunner.Result result) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("id", query.id);
        row.put("prefix", query.prefixLength);
        row.put("labels", result.labels);
        row.put("ms", Math.round(result.ms * 10) / 10.0);
        row.put("rawCount", result.rawCount);
        if (result.error != null) row.put("error", result.error);
        return GSON.toJson(row);
    }

    /**
     * Opens a text editor on a file on the event thread.
     * @param project - the open project
     * @param path - file to open
     */
    private Editor openEditor(Project project, Path path) throws Exception {
        VirtualFile vf = LocalFileSystem.getInstance().refreshAndFindFileByNioFile(path);
        if (vf == null) throw new IllegalStateException("file not found: " + path);
        Editor[] holder = new Editor[1];
        QueryRunner.onEdt(() -> holder[0] = FileEditorManager.getInstance(project).openTextEditor(new OpenFileDescriptor(project, vf, 0), false));
        if (holder[0] == null) throw new IllegalStateException("no editor for " + path);
        return holder[0];
    }

    /**
     * Closes the editor of a file.
     * @param project - the open project
     * @param path - file to close
     */
    private void closeEditor(Project project, Path path) throws Exception {
        VirtualFile vf = LocalFileSystem.getInstance().findFileByNioFile(path);
        if (vf != null) QueryRunner.onEdt(() -> FileEditorManager.getInstance(project).closeFile(vf));
    }

    /**
     * Writes a line to the log file and standard output.
     * @param message - the text to log
     */
    private void log(String message) {
        String line = java.time.LocalTime.now() + " " + message;
        System.out.println("[completionEval] " + line);
        if (logFile == null) return;
        try {
            Files.writeString(logFile, line + System.lineSeparator(), StandardCharsets.UTF_8, StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (Exception ignored) {
        }
    }

    /**
     * Renders a throwable with its stack.
     * @param t - the throwable
     */
    private static String stack(Throwable t) {
        StringWriter sw = new StringWriter();
        t.printStackTrace(new PrintWriter(sw));
        return sw.toString();
    }
}
