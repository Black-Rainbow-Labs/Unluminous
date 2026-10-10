package org.unluminous.completioneval;

import com.intellij.completion.ml.settings.CompletionMLRankingSettings;

import java.util.Map;
import java.util.TreeMap;

/** Switches the machine learning completion ranking on or off. Loaded only when its plugin is present. */
final class MlConfig {
    private MlConfig() {
    }

    /**
     * Sets the master ranking switch and reports the per language state that results.
     * @param enabled - true for the default IntelliJ configuration, false to turn ranking off
     */
    static String apply(boolean enabled) {
        CompletionMLRankingSettings settings = CompletionMLRankingSettings.getInstance();
        if (!enabled) settings.setRankingEnabled(false);
        Map<String, Boolean> languages = new TreeMap<>(settings.getState().language2state);
        return "rankingEnabled=" + settings.isRankingEnabled() + " languages=" + languages;
    }

    /**
     * Says whether ranking is enabled for the named ranker language.
     * @param language - ranker language name such as Rust or TypeScript
     */
    static boolean languageEnabled(String language) {
        CompletionMLRankingSettings settings = CompletionMLRankingSettings.getInstance();
        return settings.isRankingEnabled() && settings.isLanguageEnabled(language);
    }
}
