package org.unluminous.completioneval;

import java.util.ArrayList;
import java.util.List;

/** One completion query: a position, a prefix length, and the document edits that set it up. */
final class Query {
    /** A text replacement in UTF-16 offsets of the original document. */
    static final class Edit {
        final int start;
        final int end;
        final String text;

        Edit(int start, int end, String text) {
            this.start = start;
            this.end = end;
            this.text = text;
        }
    }

    final String id;
    final String path;
    final int prefixLength;
    final String expected;
    /** Position class from positions.json, such as member or keyword. */
    String positionClass = "";
    /** Edits in descending start order, so applying them one after another keeps offsets valid. */
    final List<Edit> edits = new ArrayList<>();
    /** Caret offset in the edited document. */
    int caret;

    Query(String id, String path, int prefixLength, String expected) {
        this.id = id;
        this.path = path;
        this.prefixLength = prefixLength;
        this.expected = expected;
    }
}
