package org.unluminous.completioneval;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** Reads positions.json and turns each position into queries in UTF-16 document offsets. */
final class Positions {
    private Positions() {
    }

    /**
     * Builds every query of one corpus, grouped by file in the order files first appear.
     * @param positionsFile - path of positions.json
     * @param corpus - corpus name to keep
     * @param split - tune, held or all
     * @param limit - maximum positions per corpus, or 0 for no cap
     * @param corpusDir - folder holding the corpus copy the IDE opens
     */
    static List<Query> load(Path positionsFile, String corpus, String split, int limit, Path corpusDir) throws IOException {
        JsonObject root = JsonParser.parseString(Files.readString(positionsFile, StandardCharsets.UTF_8)).getAsJsonObject();
        JsonArray all = root.getAsJsonArray("positions");
        Map<String, List<JsonObject>> byFile = new java.util.LinkedHashMap<>();
        int kept = 0;
        for (JsonElement element : all) {
            JsonObject position = element.getAsJsonObject();
            if (!keeps(position, corpus, split)) continue;
            if (limit > 0 && kept >= limit) break;
            kept++;
            byFile.computeIfAbsent(position.get("path").getAsString(), k -> new ArrayList<>()).add(position);
        }
        List<Query> queries = new ArrayList<>();
        for (Map.Entry<String, List<JsonObject>> file : byFile.entrySet()) {
            addFileQueries(queries, corpusDir, file.getKey(), file.getValue());
        }
        return queries;
    }

    /**
     * Says whether a position belongs to the corpus and split being run.
     * @param position - one entry of positions.json
     * @param corpus - corpus name wanted
     * @param split - tune, held or all
     */
    private static boolean keeps(JsonObject position, String corpus, String split) {
        if (!position.get("corpus").getAsString().equals(corpus)) return false;
        return split.equals("all") || position.get("split").getAsString().equals(split);
    }

    /**
     * Expands the positions of one file into queries, one for each prefix length 0 to 3.
     * @param out - list the queries are appended to
     * @param corpusDir - corpus folder
     * @param path - file path relative to the corpus root
     * @param positions - the positions inside that file
     */
    private static void addFileQueries(List<Query> out, Path corpusDir, String path, List<JsonObject> positions) throws IOException {
        byte[] raw = Files.readAllBytes(corpusDir.resolve(path));
        String text = new String(raw, StandardCharsets.UTF_8).replace("\r\n", "\n");
        byte[] bytes = text.getBytes(StandardCharsets.UTF_8);
        int[] byteToChar = byteToCharTable(text, bytes.length);
        for (JsonObject position : positions) {
            for (int prefix = 0; prefix <= 3; prefix++) {
                Query query = buildQuery(position, prefix, byteToChar);
                if (query != null) out.add(query);
            }
        }
    }

    /**
     * Maps every UTF-8 byte offset of the text to its UTF-16 offset.
     * @param text - the normalised file text
     * @param byteLength - length of the text in UTF-8 bytes
     */
    private static int[] byteToCharTable(String text, int byteLength) {
        int[] table = new int[byteLength + 2];
        int bytePos = 0;
        int i = 0;
        while (i < text.length()) {
            int cp = text.codePointAt(i);
            int units = Character.charCount(cp);
            int width = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
            for (int b = 0; b < width; b++) table[bytePos + b] = i;
            bytePos += width;
            i += units;
        }
        table[bytePos] = i;
        table[bytePos + 1] = i;
        return table;
    }

    /**
     * Builds one query, or null when the prefix is longer than the identifier.
     * @param position - the position being queried
     * @param prefix - number of characters of the identifier to keep
     * @param byteToChar - UTF-8 byte offset to UTF-16 offset table of the file
     */
    private static Query buildQuery(JsonObject position, int prefix, int[] byteToChar) {
        String expected = position.get("expected").getAsString();
        if (prefix > expected.codePointCount(0, expected.length())) return null;
        String kept = expected.substring(0, expected.offsetByCodePoints(0, prefix));
        Query query = new Query(position.get("id").getAsString(), position.get("path").getAsString(), prefix, expected);
        query.positionClass = position.has("class") ? position.get("class").getAsString() : "";
        int start = byteToChar[position.get("start").getAsInt()];
        int end = byteToChar[position.get("end").getAsInt()];
        List<Query.Edit> edits = new ArrayList<>();
        edits.add(new Query.Edit(start, end, kept));
        int removedBefore = 0;
        if (position.has("remove")) {
            for (JsonElement r : position.getAsJsonArray("remove")) {
                int rs = byteToChar[r.getAsJsonObject().get("start").getAsInt()];
                int re = byteToChar[r.getAsJsonObject().get("end").getAsInt()];
                edits.add(new Query.Edit(rs, re, ""));
                if (re <= start) removedBefore += re - rs;
            }
        }
        edits.sort((a, b) -> Integer.compare(b.start, a.start));
        query.edits.addAll(edits);
        query.caret = start - removedBefore + kept.length();
        return query;
    }

    /** Groups queries by file so one editor serves a run of queries. */
    static Map<String, List<Query>> byFile(List<Query> queries) {
        Map<String, List<Query>> map = new java.util.LinkedHashMap<>();
        for (Query q : queries) map.computeIfAbsent(q.path, k -> new ArrayList<>()).add(q);
        return map;
    }

}
