package org.unluminous.completioneval;

import com.intellij.codeInsight.completion.CodeCompletionHandlerBase;
import com.intellij.codeInsight.completion.CompletionPhase;
import com.intellij.codeInsight.completion.CompletionType;
import com.intellij.codeInsight.completion.impl.CompletionServiceImpl;
import com.intellij.codeInsight.lookup.LookupElement;
import com.intellij.codeInsight.lookup.LookupManager;
import com.intellij.codeInsight.lookup.impl.LookupImpl;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.application.ModalityState;
import com.intellij.openapi.command.WriteCommandAction;
import com.intellij.openapi.editor.Document;
import com.intellij.openapi.editor.Editor;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.util.TextRange;
import com.intellij.psi.PsiDocumentManager;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

/** Runs one completion query against an open editor and puts the document back afterwards. */
final class QueryRunner {
    /** What one query produced. */
    static final class Result {
        List<String> labels = new ArrayList<>();
        int rawCount;
        double ms;
        String error;
    }

    private static final int MAX_LABELS = 50;
    private static final long WAIT_MILLIS = 10_000;

    private static final boolean TRACE = Boolean.getBoolean("completionEval.trace");
    private final Project project;
    private String lastPhase = "";

    QueryRunner(Project project) {
        this.project = project;
    }

    /**
     * Applies the query's edits, asks for basic completion, reads the lookup and restores the text.
     * @param editor - editor on the file the query is about
     * @param query - the query to run
     */
    Result run(Editor editor, Query query) {
        Result result = new Result();
        Document document = editor.getDocument();
        String original = document.getText();
        List<Object[]> undo = new ArrayList<>();
        try {
            onEdt(() -> applyEdits(document, query, undo));
            result.ms = complete(editor, query, result);
        } catch (Throwable t) {
            result.error = t.getClass().getSimpleName() + ": " + t.getMessage();
        } finally {
            try {
                onEdt(() -> restore(editor, document, original, undo));
            } catch (Throwable t) {
                result.error = (result.error == null ? "" : result.error + " | ") + "restore failed: " + t;
            }
        }
        return result;
    }

    /**
     * Runs completion, waits for the lookup to settle and reads its items in display order.
     * @param editor - the editor holding the edited document
     * @param query - the query, for its caret offset
     * @param result - receives the labels
     */
    private double complete(Editor editor, Query query, Result result) throws Exception {
        long started = System.nanoTime();
        onEdt(() -> invokeCompletion(editor, query.caret));
        long deadline = System.currentTimeMillis() + WAIT_MILLIS;
        while (System.currentTimeMillis() < deadline && isCalculating(editor)) Thread.sleep(5);
        if (TRACE) System.out.println("[completionEval] settled phase=" + lastPhase);
        List<String> raw = readItems(editor);
        double ms = (System.nanoTime() - started) / 1_000_000.0;
        result.rawCount = raw.size();
        Set<String> distinct = new LinkedHashSet<>(raw);
        for (String label : distinct) {
            if (result.labels.size() >= MAX_LABELS) break;
            result.labels.add(label);
        }
        return ms;
    }

    /**
     * Moves the caret, commits the document and invokes basic completion as Ctrl+Space would.
     * @param editor - the editor
     * @param caret - caret offset
     */
    private void invokeCompletion(Editor editor, int caret) {
        editor.getCaretModel().moveToOffset(caret);
        PsiDocumentManager.getInstance(project).commitAllDocuments();
        CodeCompletionHandlerBase handler = CodeCompletionHandlerBase.createHandler(CompletionType.BASIC, true, false, false);
        handler.invokeCompletion(project, editor, 1);
    }

    /**
     * Says whether the active lookup is still computing items.
     * @param editor - the editor
     */
    private boolean isCalculating(Editor editor) throws Exception {
        boolean[] calculating = new boolean[1];
        onEdt(() -> {
            LookupImpl lookup = (LookupImpl) LookupManager.getActiveLookup(editor);
            CompletionPhase phase = CompletionServiceImpl.getCompletionPhase();
            String name = phase.getClass().getSimpleName();
            if (TRACE && !name.equals(lastPhase)) System.out.println("[completionEval] phase " + name);
            lastPhase = name;
            boolean running = name.equals("CommittingDocuments") || name.equals("BgCalculation") || name.equals("Synchronous");
            calculating[0] = running || (lookup != null && lookup.isCalculating());
        });
        return calculating[0];
    }

    /**
     * Reads the lookup strings of the active lookup in display order and hides the lookup.
     * @param editor - the editor
     */
    private List<String> readItems(Editor editor) throws Exception {
        List<String> labels = new ArrayList<>();
        onEdt(() -> {
            LookupImpl lookup = (LookupImpl) LookupManager.getActiveLookup(editor);
            if (lookup != null) {
                for (LookupElement item : lookup.getItems()) labels.add(item.getLookupString());
            }
            LookupManager.getInstance(project).hideActiveLookup();
        });
        return labels;
    }

    /**
     * Applies the query's edits in a write command and records how to take each one back.
     * @param document - the editor's document
     * @param query - the query holding the edits
     * @param undo - receives start, end and old text of each applied edit
     */
    private void applyEdits(Document document, Query query, List<Object[]> undo) {
        WriteCommandAction.runWriteCommandAction(project, () -> {
            for (Query.Edit edit : query.edits) {
                String old = document.getText(new TextRange(edit.start, edit.end));
                document.replaceString(edit.start, edit.end, edit.text);
                undo.add(new Object[] {edit.start, edit.start + edit.text.length(), old});
            }
        });
        PsiDocumentManager.getInstance(project).commitDocument(document);
    }

    /**
     * Takes every edit back in reverse order and checks the text equals the original.
     * @param editor - the editor
     * @param document - the editor's document
     * @param original - the text before the query
     * @param undo - the records made by applyEdits
     */
    private void restore(Editor editor, Document document, String original, List<Object[]> undo) {
        LookupManager.getInstance(project).hideActiveLookup();
        WriteCommandAction.runWriteCommandAction(project, () -> {
            for (int i = undo.size() - 1; i >= 0; i--) {
                Object[] u = undo.get(i);
                document.replaceString((Integer) u[0], (Integer) u[1], (String) u[2]);
            }
            if (!document.getText().equals(original)) document.setText(original);
        });
        PsiDocumentManager.getInstance(project).commitDocument(document);
        editor.getCaretModel().moveToOffset(0);
    }

    /**
     * Runs work on the event dispatch thread and waits for it.
     * @param work - the work to run
     */
    static void onEdt(Runnable work) throws Exception {
        Throwable[] failure = new Throwable[1];
        ApplicationManager.getApplication().invokeAndWait(() -> {
            try {
                work.run();
            } catch (Throwable t) {
                failure[0] = t;
            }
        }, ModalityState.nonModal());
        if (failure[0] instanceof Exception) throw (Exception) failure[0];
        if (failure[0] instanceof Error) throw (Error) failure[0];
    }
}
