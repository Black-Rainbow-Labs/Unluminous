"""The features the completion ranking reads, worked out from a pool row exactly as
`unluminous_core::completion::model::features` works them out from a `Row` (`task-2237`).

Every value is a float made from integers, so the Rust side and this side compute the same numbers and
the trees split them the same way. A test in `crates/unluminous-core` checks a set of rows this file
scored against the Rust code.
"""

# Which source each bit of `offeredBy` is, in the order of `completion::Source`.
SOURCES = ['this file', 'word', 'open tab', 'project', 'language', 'module', 'kernel', 'member', 'needs import', 'server']
# `MatchClass` in order.
CLASSES = ['exact', 'prefix', 'humps', 'word start', 'subsequence']
# `Locality` in order.
LOCALITIES = ['Receiver', 'Local', 'ThisFile', 'OpenTab', 'SameFolder', 'Package', 'Project', 'NeedsImport', 'Dependency', 'Language']
# `Place` in order.
PLACES = ['unknown', 'statement', 'expression', 'type', 'pattern', 'import', 'argument', 'member']
# `Kind::name` values, in the order of `Kind::ALL`. A row with no kind is `len(KINDS)`.
KINDS = ['function', 'method', 'type', 'struct', 'enum', 'variant', 'trait', 'interface', 'class', 'field',
         'constant', 'variable', 'module', 'parameter', 'alias', 'macro', 'keyword', 'snippet']

# A distance with no place to measure to.
FAR = 100000.0

NAMES = [
    'stem_length', 'case_agrees', 'expected_type', 'preselect', 'deprecated', 'locality', 'place_fit', 'place',
    'kind', 'source', 'sources', 'server_score', 'language', 'uses_here', 'lines_above',
    'lines_below', 'uses_near', 'same_before', 'same_after', 'words_nearby', 'score', 'length', 'left_to_type',
    'chain_rank', 'is_this_file', 'is_word', 'is_open_tab', 'is_index', 'is_language', 'is_module', 'is_member',
    'is_import', 'is_server', 'before', 'after',
]

LANGUAGES = {'rust': 0, 'typescript': 1}


def kind_index(kind):
    return KINDS.index(kind) if kind in KINDS else len(KINDS)


def server_score(order, language):
    """A server's own judgement as one number, larger better: rust-analyzer's relevance (its sort text is
    the score XOR 0xFFFFFFFF, around 2**31) as an offset from its base score, and tsserver's group from
    "10" for locals to "18", negated. -1000 for a row no server offered."""
    if order is None or order > 0xFFFFFFFF:
        return -1000.0
    if order > 0xFFFF:
        return float((0xFFFFFFFF - order) - 0x7FFFFFFF)
    return -float(order)


def chain_rank(row):
    """Where the chain of weighers put the row: `chainRank` when the window said, otherwise its place in
    the window's order, which was the chain's in a build with no model."""
    return row.get('chainRank', row['at'])


def features(row, stem, place, language, tokens=(0, 0)):
    """The feature vector of one row.

    @param row - the pool row, as `editor complete --explain` wrote it
    @param stem - what has been typed
    @param place - the place's name
    @param language - the language's name
    @param tokens - the classes of the tokens before and after the word (`completion::token_class`)
    """
    name = row['name']
    first = stem[:1]
    case_agrees = 1.0
    if first and first.isupper() and name[:1].isalpha():
        case_agrees = 1.0 if name[:1].isupper() else 0.0
    bits = 0
    for i, source in enumerate(SOURCES):
        if source in row.get('offeredBy', []):
            bits |= 1 << i
    above = row.get('linesAbove')
    below = row.get('linesBelow')
    length = row['length']
    return [
        float(len(stem)),
        case_agrees,
        1.0 if row.get('expectedType') else 0.0,
        1.0 if row.get('preselect') else 0.0,
        1.0 if row.get('deprecated') else 0.0,
        float(LOCALITIES.index(row['locality'])),
        float(row['placeFit']),
        float(PLACES.index(place)),
        float(kind_index(row.get('kind'))),
        float(SOURCES.index(row['source'])),
        float(bin(bits).count('1')),
        server_score(row.get('serverOrder'), language),
        float(LANGUAGES.get(language, 2)),
        float(row['usesHere']),
        FAR if above is None else float(above),
        FAR if below is None else float(below),
        float(row['usesNear']),
        float(row['sameBefore']),
        float(row['sameAfter']),
        float(row['wordsNearby']),
        float(row['score']),
        float(length),
        float(length - len(stem)),
        float(chain_rank(row)),
    ] + [1.0 if (bits >> i) & 1 else 0.0 for i in (0, 1, 2, 3, 4, 5, 7, 8, 9)] + [float(tokens[0]), float(tokens[1])]
