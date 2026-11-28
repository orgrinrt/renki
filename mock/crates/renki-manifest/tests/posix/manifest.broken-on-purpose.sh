# SPDX-License-Identifier: MPL-2.0
#
# A reader of manifests in POSIX sh and awk, for a tool that cannot link Rust.
# Source it, then:
#
#   manifest dump <file>         every leaf of the merged view, one per line
#   manifest get <file> <key>    the dump's line for one key
#
# <file> is the base, name.ext. Its layers name.<word>.ext sit beside it or are
# reached through meta.overlays.include, and are read, ordered and merged as
# the Rust reader reads them. A line of the dump is `<dotted path> = <value>`,
# an array of tables indexed as name[0], the value in one canonical TOML form,
# the lines in byte order of the path. A manifest this reader cannot read is
# refused with one line on the error stream naming the file, and the line
# where there is one, and the answer is exit 1. A key no layer holds is exit 1
# with nothing written.
#
# A shell predicate runs its command, through /bin/sh -c in the base's directory,
# on every dump and every get that reads a layer it decides. Nothing is kept
# between calls, so a command with effects has them on each read. What the
# command prints goes to /dev/null; what it writes on its error stream is the
# caller's.
#
# The TOML it reads: comments, trailing ones included; [table], [dotted.table]
# and [[array.of.tables]] headers; bare and dotted keys; basic strings with
# \", \\, \n and \t; literal strings; integers; booleans; arrays of those, on
# one line or several. It refuses multi-line strings, inline tables, nested
# arrays, quoted keys, floats, dates and times, and every other escape, and
# what is no TOML inside those forms: a control byte other than a tab, and
# bytes that are no UTF-8.
#
# Every path is resolved by the file system, a link and a .. after it
# included, never as text. A path holding a newline is refused, since no line
# can name it.

manifest() {
	case ${1-} in
	dump) [ "$#" -eq 2 ] || set -- ;;
	get) [ "$#" -eq 3 ] || set -- ;;
	*) set -- ;;
	esac
	if [ "$#" -eq 0 ]; then
		echo 'usage: manifest dump <file> | manifest get <file> <key>' >&2
		return 2
	fi
	LC_ALL=C awk "$_renki_manifest_awk" "$1" "$2" "${3-}" "${LC_ALL+set}" "${LC_ALL-}"
}

_renki_manifest_awk='
BEGIN {
	VERB = ARGV[1]; BASE = ARGV[2]; WANT = ARGV[3]
	LCSET = ARGV[4]; LCVAL = ARGV[5]
	ARGC = 1
	Q = "\047"
	SUBSET = " is outside the POSIX reader\047s TOML subset"
	# Byte classes built from the bytes themselves, since an awk may read an
	# octal escape inside a bracket expression its own way.
	CTRL = "[" byte(1) "-" byte(8) byte(10) "-" byte(12) byte(14) "-" byte(31) byte(127) "]"
	for (i = 1; i < 32; i++) ORD[byte(i)] = i
	ORD[byte(127)] = 127
	HIGH = "[" byte(128) "-" byte(255) "]"
	CONT = "[" byte(128) "-" byte(191) "]"
	UTF8 = "[" byte(194) "-" byte(223) "]" CONT "|" \
		byte(224) "[" byte(160) "-" byte(191) "]" CONT "|" \
		"[" byte(225) "-" byte(236) byte(238) byte(239) "]" CONT CONT "|" \
		byte(237) "[" byte(128) "-" byte(159) "]" CONT "|" \
		byte(240) "[" byte(144) "-" byte(191) "]" CONT CONT "|" \
		"[" byte(241) "-" byte(243) "]" CONT CONT CONT "|" \
		byte(244) "[" byte(128) "-" byte(143) "]" CONT CONT
	main()
	exit 0
}

function byte(n) { return sprintf("%c", n) }

function refuse(msg) {
	print msg | "cat 1>&2"
	close("cat 1>&2")
	exit 1
}

# --- paths --------------------------------------------------------------------

function lastslash(p,   i, j) {
	i = 0
	while ((j = index(substr(p, i + 1), "/")) > 0) i += j
	return i
}

function dirof(p,   i) {
	i = lastslash(p)
	if (i == 1) return "/"
	return i ? substr(p, 1, i - 1) : ""
}

function baseof(p) { return substr(p, lastslash(p) + 1) }

function join2(dir, name) { return dir == "" ? name : (dir == "/" ? "/" name : dir "/" name) }

# An include entry joined to the directory of the file naming it, as text. A
# . or .. in it is left for the file system to resolve, after any link.
function joinlabel(dir, rel) {
	if (substr(rel, 1, 1) == "/") return rel
	return join2(dir, rel)
}

# Where a label is opened from: labels are relative to the base.
function fpath(label) {
	if (substr(label, 1, 1) == "/") return label
	if (BDIR == "") return label
	return join2(BDIR, label)
}

function dpath(dirlabel) {
	if (dirlabel == "") return BDIR == "" ? "." : BDIR
	return fpath(dirlabel)
}

function q(s,   out, i) {
	out = ""
	while ((i = index(s, Q)) > 0) {
		out = out substr(s, 1, i - 1) Q "\\" Q Q
		s = substr(s, i + 1)
	}
	return Q out s Q
}

function nl(s) { return index(s, "\n") > 0 }

# Everything a command prints, its lines joined again by newlines.
function output(cmd,   l, out, n) {
	out = ""; n = 0
	while ((cmd | getline l) > 0) out = (n++ ? out "\n" : "") l
	close(cmd)
	return out
}

# The canonical path of a file, its links followed and its directory resolved
# by cd -P; or "-missing" where there is no file to read, and "-notregular"
# where what is there is no regular file. A path holding a newline comes back
# holding it.
function canonical(label) {
	return output("f=" q(fpath(label)) "; n=0; " \
		"while [ -h \"$f\" ]; do n=$((n + 1)); [ $n -le 40 ] || { echo -missing; exit 0; }; " \
		"case $f in */*) d=${f%/*}/ ;; *) d= ;; esac; " \
		"t=$(unset QUOTING_STYLE; ls -ld -- \"$f\" && echo x) || { echo -missing; exit 0; }; t=${t%??}; t=${t#*\" $f -> \"}; " \
		"case $t in /*) f=$t ;; *) f=$d$t ;; esac; done; " \
		"[ -e \"$f\" ] || { echo -missing; exit 0; }; [ -f \"$f\" ] || { echo -notregular; exit 0; }; " \
		"case $f in */*) d=${f%/*}; [ -n \"$d\" ] || d=/ ;; *) d=. ;; esac; " \
		"d=$(cd -P -- \"$d\" && pwd -P && echo x) || { echo -missing; exit 0; }; d=${d%??}; " \
		"printf \"%s/%s\\n\" \"${d%/}\" \"${f##*/}\"")
}

# Every entry of a directory but . and .., in byte order, into ent; -2 where
# the directory is none, and -1 where it cannot be listed. Each name is
# followed by a line holding only /, which no name holds, so a name holding a
# newline comes back whole.
function listdir(dirlabel, ent,   cmd, l, n, acc, lines, i, j, t) {
	cmd = "d=" q(dpath(dirlabel)) "; [ -d \"$d\" ] || { echo -nodir; exit 0; }; " \
		"cd -P -- \"$d\" 2>/dev/null && ls -a . >/dev/null 2>&1 || { echo -unlistable; exit 0; }; " \
		"for f in .* *; do case $f in .|..) continue ;; esac; " \
		"[ -e \"$f\" ] || [ -h \"$f\" ] || continue; printf \"%s\\n/\\n\" \"$f\"; done"
	n = 0; lines = 0; acc = ""
	while ((cmd | getline l) > 0) {
		if (l == "/" && lines) { ent[++n] = acc; acc = ""; lines = 0; continue }
		acc = (lines++ ? acc "\n" : "") l
	}
	close(cmd)
	if (lines) return acc == "-nodir" ? -2 : -1
	for (i = 2; i <= n; i++) {
		t = ent[i]
		for (j = i - 1; j > 0 && (ent[j] "") > (t ""); j--) ent[j + 1] = ent[j]
		ent[j + 1] = t
	}
	return n
}

# The shell rule: * and ? within one segment, [...] with ! negating it, and a
# . that begins the name matched by a . in the pattern alone.
function glob(p, n,   lp, ln, pi, ni, back, bstar, btaken, c, step, r) {
	if (substr(n, 1, 1) == "." && substr(p, 1, 1) != ".") return 0
	lp = length(p); ln = length(n); pi = 1; ni = 1; back = 0
	while (ni <= ln) {
		step = 0
		if (pi <= lp) {
			c = substr(p, pi, 1)
			if (c == "*") { back = 1; bstar = pi; btaken = ni; pi++; continue }
			if (c == "?") { if (substr(n, ni, 1) != "/") step = pi + 1 }
			else if (c == "[") {
				r = bracket(p, pi, substr(n, ni, 1))
				if (r > 0) step = r
				else if (r < 0 && substr(n, ni, 1) == "[") step = pi + 1
			}
			else if (c == substr(n, ni, 1)) step = pi + 1
		}
		if (step) { pi = step; ni++ }
		else if (back && substr(n, btaken, 1) != "/") { btaken++; pi = bstar + 1; ni = btaken }
		else return 0
	}
	while (pi <= lp && substr(p, pi, 1) == "*") pi++
	return pi > lp
}

function bracket(p, at, c,   lp, i, neg, first, hit) {
	lp = length(p); i = at + 1
	neg = (i <= lp && substr(p, i, 1) == "!")
	if (neg) i++
	first = i; hit = 0
	while (i <= lp) {
		if (substr(p, i, 1) == "]" && i > first) return (hit != neg && c != "/") ? i + 1 : 0
		if (i + 2 <= lp && substr(p, i + 1, 1) == "-" && substr(p, i + 2, 1) != "]") {
			if (substr(p, i, 1) <= c && c <= substr(p, i + 2, 1)) hit = 1
			i += 3
		} else {
			if (substr(p, i, 1) == c) hit = 1
			i++
		}
	}
	return -1
}

# --- reading one file ---------------------------------------------------------

function here() { return PLAB ":" LNUM ": " }

function beyond(what) { refuse(here() what SUBSET) }

# The refusals of a document that is no TOML are in the words of the parser.
function expected(what) { refuse(here() "expected " what) }

function extend() { refuse(here() "a key path through something it may not extend") }

# A carriage return outside a string or a comment, with no line feed after
# it; one inside a string or a comment is a control character, as the parser
# reads them.
function barecr() { refuse(here() "a carriage return with no line feed after it") }

# A comment from I to the end of the line.
function comment() { if (index(substr(S, I), "\r")) refuse(here() "a control character") }

# What may follow a value or a header on its line: nothing, or a comment.
function end() {
	if (I > length(S)) return
	if (ch() == "\r") barecr()
	if (ch() == "#") comment()
	else expected("newline")
}

# The whole file into FL, its lines without their newlines, with FN of them
# and FEND the line its end is on. The file is read through tr, which gives a
# NUL byte as another control byte, so no awk ever holds one, and its exit is
# read after the last line, so a file that cannot be read is told from an
# empty one. The parser refuses bytes that are no UTF-8 before anything else,
# and so does this.
function load(   cmd, l, n, st) {
	cmd = "{ tr " q("\\000") " " q("\\001") " 2>/dev/null < " q(PPATH) " && echo; } 2>/dev/null; echo \"/$?\""
	n = 0
	while ((cmd | getline l) > 0) FL[++n] = l
	close(cmd)
	st = n ? FL[n] : ""
	if (st != "/0") refuse(PLAB " cannot be read")
	n--
	# the echo ends the last line; where the file ended in a newline, it adds
	# an empty one of its own
	FEND = n
	if (n > 0 && FL[n] == "") { n--; FEND = n + 1 }
	FN = n
	for (l = 1; l <= FN; l++)
		if (FL[l] ~ HIGH && !utf8(FL[l])) { LNUM = l; refuse(here() "the text is not UTF-8") }
	FI = 0
}

function nextline() {
	if (FI >= FN) { LNUM = FEND; S = ""; I = 1; return 0 }
	S = FL[++FI]
	LNUM = FI
	sub(/\r$/, "", S)
	I = 1
	if (S ~ CTRL) refuse(here() "a control character")
	return 1
}

# Whether every byte above 127 of s is part of a UTF-8 sequence: no stray
# continuation, no overlong form, no surrogate and nothing above U+10FFFF.
function utf8(s) {
	gsub(UTF8, "", s)
	return s !~ HIGH
}

function ch() { return substr(S, I, 1) }

function ws(   c) {
	while ((c = substr(S, I, 1)) == " " || c == "\t") I++
}

function join(p, k) { return p == "" ? k : p "." k }

function node(L, p, shape) {
	SH[L, p] = shape
	LN[L, p] = LNUM
	NODES[L, ++NN[L]] = p
}

function parse(L,   cur) {
	PLAB = LAB[L]; PPATH = fpath(PLAB); LNUM = 0; NN[L] = 0; cur = ""
	load()
	while (nextline()) {
		ws()
		if (I > length(S)) continue
		if (ch() == "#") { comment(); continue }
		if (ch() == "\r") barecr()
		if (ch() == "[") cur = header(L)
		else keyval(L, cur)
	}
}

function keypath(K,   n, c, start) {
	n = 0
	while (1) {
		ws()
		c = ch()
		if (c == "\"" || c == Q) beyond("a quoted key")
		start = I
		while (substr(S, I, 1) ~ /^[A-Za-z0-9_-]$/) I++
		if (I == start) expected("key")
		K[++n] = substr(S, start, I - start)
		ws()
		if (ch() != ".") return n
		I++
	}
}

# The table a header names, found through the last element of each array of
# tables on the way.
function prefix(L, K, nk, how,   p, i) {
	p = ""
	for (i = 1; i < nk; i++) {
		p = join(p, K[i])
		if (!((L, p) in SH)) { node(L, p, "t"); DEF[L, p] = how }
		else if (SH[L, p] == "a" && how != "d") p = p "[" (NI[L, p] - 1) "]"
		else if (SH[L, p] != "t" || (how == "d" && DEF[L, p] != "d")) extend()
	}
	return join(p, K[nk])
}

function header(L,   arr, K, nk, p, e) {
	arr = substr(S, I, 2) == "[["
	I += arr ? 2 : 1
	split("", K)
	nk = keypath(K)
	ws()
	if (arr) { if (substr(S, I, 2) != "]]") expected("]]"); I += 2 }
	else { if (ch() != "]") expected("]"); I++ }
	ws()
	end()
	p = prefix(L, K, nk, "i")
	if (!arr) {
		if (!((L, p) in SH)) { node(L, p, "t"); DEF[L, p] = "h" }
		else if (SH[L, p] == "t" && DEF[L, p] == "i") { DEF[L, p] = "h"; LN[L, p] = LNUM }
		else if (SH[L, p] == "t" && DEF[L, p] == "h") refuse(here() "a table defined twice")
		else extend()
		return p
	}
	if (!((L, p) in SH)) { node(L, p, "a"); NI[L, p] = 0 }
	else if (SH[L, p] != "a") extend()
	e = p "[" NI[L, p] "]"
	NI[L, p]++
	node(L, e, "t")
	DEF[L, e] = "h"
	return e
}

function keyval(L, cur,   K, nk, i, p) {
	split("", K)
	nk = keypath(K)
	ws()
	if (ch() != "=") expected("=")
	I++
	for (i = 1; i <= nk; i++) K[i] = (i == 1 && cur != "") ? cur "." K[i] : K[i]
	p = prefix(L, K, nk, "d")
	if ((L, p) in SH) refuse(here() "a key defined twice")
	value(L, p)
	ws()
	end()
}

function value(L, p,   c) {
	ws()
	c = ch()
	if (c == "" || c == "\r") expected("value")
	if (c == "[") { list(L, p); return }
	if (c == "{") beyond("an inline table")
	scalar()
	node(L, p, "s")
	VAL[L, p] = SV; TY[L, p] = ST; STR[L, p] = SS
}

# The rest of an array, across lines, up to what follows; at the end of the
# file it is refused as the parser does, wanting what stands next.
function fill(want) {
	while (1) {
		ws()
		if (ch() == "\r") barecr()
		if (I <= length(S) && ch() != "#") return
		if (I <= length(S)) comment()
		if (!nextline()) expected(want)
	}
}

function list(L, p,   n, c) {
	node(L, p, "l")
	I++
	n = 0
	while (1) {
		fill("value")
		c = ch()
		if (c == "]") { I++; break }
		if (c == "[") beyond("a nested array")
		if (c == "{") beyond("an inline table")
		scalar()
		n++
		IT[L, p, n] = SV; ITY[L, p, n] = ST; ISTR[L, p, n] = SS
		fill(", or ]")
		c = ch()
		if (c == ",") { I++; continue }
		if (c == "]") { I++; break }
		expected(", or ]")
	}
	NI[L, p] = n
}

# A text as a basic string, in the dump and in a refusal alike. A string the
# subset reads never holds a carriage return; the value of a variable may.
function canon(s,   out, i, c) {
	out = ""
	for (i = 1; i <= length(s); i++) {
		c = substr(s, i, 1)
		if (c == "\\") c = "\\\\"
		else if (c == "\"") c = "\\\""
		else if (c == "\n") c = "\\n"
		else if (c == "\t") c = "\\t"
		else if (c == "\r") c = "\\r"
		else if (c in ORD) c = sprintf("\\u%04X", ORD[c])
		out = out c
	}
	return "\"" out "\""
}

# One scalar at I, into SV (its canonical text), ST (its type) and SS (a
# string decoded).
function scalar(   c, start, tok) {
	c = ch()
	if (c == "\"") {
		if (substr(S, I, 3) == "\"\"\"") beyond("a multi-line string")
		basic()
		return
	}
	if (c == Q) {
		if (substr(S, I, 3) == Q Q Q) beyond("a multi-line string")
		literal()
		return
	}
	start = I
	while (I <= length(S) && (c = ch()) != " " && c != "\t" && c != "," && c != "]" && c != "#" && c != "\r") I++
	tok = substr(S, start, I - start)
	SS = ""
	if (tok == "") expected("value")
	if (tok == "true" || tok == "false") { ST = "bool"; SV = tok; return }
	if (tok ~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ || tok ~ /^[0-9][0-9]:[0-9][0-9]/) beyond("a date or time")
	if (tok ~ /^[+-]?(inf|nan)$/) beyond("a float")
	if (tok !~ /^0[xob]/ && tok ~ /^[+-]?[0-9][0-9_]*([.][0-9_]+)?([eE][+-]?[0-9_]+)?$/ && tok ~ /[.eE]/) beyond("a float")
	ST = "int"
	SV = integer(tok)
}

function basic(   out, c, e) {
	I++
	out = ""
	while (1) {
		if (I > length(S)) refuse(here() "a string with no end")
		c = ch()
		if (c == "\"") { I++; break }
		if (c == "\r") refuse(here() "a control character")
		if (c == "\\") {
			e = substr(S, I + 1, 1)
			if (e == "\"") out = out "\""
			else if (e == "\\") out = out "\\"
			else if (e == "n") out = out "\n"
			else if (e == "t") out = out "\t"
			else if (e == "") refuse(here() "an escape TOML does not have")
			else if (index("bfreuUx", e)) beyond("a \\" e " escape")
			else refuse(here() "an escape TOML does not have")
			I += 2
			continue
		}
		out = out c
		I++
	}
	ST = "str"; SS = out; SV = canon(out)
}

function literal(   j) {
	j = index(substr(S, I + 1), Q)
	if (index(j ? substr(S, I + 1, j - 1) : substr(S, I + 1), "\r")) refuse(here() "a control character")
	if (j == 0) refuse(here() "a string with no end")
	SS = substr(S, I + 1, j - 1)
	I += j + 1
	ST = "str"; SV = canon(SS)
}

# An integer in decimal, its sign kept only where it is below zero.
function integer(tok,   neg, d, max) {
	if (tok ~ /^[+-]?(0|[1-9](_?[0-9])*)$/) {
		neg = substr(tok, 1, 1) == "-"
		d = tok
		gsub(/[-+_]/, "", d)
		if (d == "0") neg = 0
	}
	else if (tok ~ /^0x[0-9a-fA-F](_?[0-9a-fA-F])*$/) d = based(substr(tok, 3), 16)
	else if (tok ~ /^0o[0-7](_?[0-7])*$/) d = based(substr(tok, 3), 8)
	else if (tok ~ /^0b[01](_?[01])*$/) d = based(substr(tok, 3), 2)
	else refuse(here() "not a TOML value")
	max = neg ? "9223372036854775808" : "9223372036854775807"
	if (length(d) > 19 || (length(d) == 19 && d "" > max)) refuse(here() "an integer outside the 64-bit range")
	return (neg ? "-" : "") d
}

function based(digits, b,   dec, i, c) {
	dec = "0"
	for (i = 1; i <= length(digits); i++) {
		c = tolower(substr(digits, i, 1))
		if (c != "_") dec = muladd(dec, b, index("0123456789abcdef", c) - 1)
	}
	return dec
}

function muladd(dec, m, a,   i, out, carry, t) {
	out = ""
	carry = a
	for (i = length(dec); i >= 1; i--) {
		t = substr(dec, i, 1) * m + carry
		out = (t % 10) out
		carry = int(t / 10)
	}
	while (carry > 0) { out = (carry % 10) out; carry = int(carry / 10) }
	sub(/^0+/, "", out)
	return out == "" ? "0" : out
}

# --- the meta table ------------------------------------------------------------

function ismeta(p) { return p == "meta" || substr(p, 1, 5) == "meta." || substr(p, 1, 5) == "meta[" }

function leaf(L, p) { return SH[L, p] == "s" || SH[L, p] == "l" }

function at(L, k) { return LAB[L] ":" LN[L, k] ": " }

function wrong(L, p) { refuse(at(L, p) p " holds a value of the wrong shape") }

function overlay(L, p) { refuse(at(L, p) p " is read from the base and defaults only, not from an overlay") }

# The meta keys that hold a list, each with what its items must be.
function lists(   n, ks, i) {
	n = split("meta.overlays.include string meta.overlays.order word meta.overlays.untracked word meta.replace string meta.unset string", ks, " ")
	for (i = 1; i < n; i += 2) ITEM[ks[i]] = ks[i + 1]
}

# The first thing wrong with the meta table of L, a part that is "base",
# "defaults" or "layer", taken key by key in the order the file holds them,
# each key asked for its shape, then its place, then its items. The
# predicates are checked whole once the base and defaults are read; a
# definition in an overlay is refused at its first leaf.
function defect(L, part,   i, p, k, j, seen) {
	for (i = 1; i <= NN[L]; i++) {
		p = NODES[L, i]
		if (!ismeta(p)) continue
		if (p == "meta" || p == "meta.overlays" || p == "meta.predicates") { if (SH[L, p] != "t") wrong(L, p); continue }
		if (substr(p, 1, 16) == "meta.predicates.") {
			if (part == "layer" && leaf(L, p)) overlay(L, p)
			continue
		}
		if (p == "meta.when") {
			if (!(SH[L, p] == "s" && TY[L, p] == "str")) wrong(L, p)
			if (part != "layer") refuse(at(L, p) p " is refused on the base and defaults, which are always read")
			continue
		}
		if (p in ITEM) {
			if (SH[L, p] != "l") wrong(L, p)
			if (part == "layer" && ITEM[p] == "word") overlay(L, p)
			for (j = 1; j <= NI[L, p]; j++)
				if (ITY[L, p, j] != "str") refuse(at(L, p) p " holds " got(ITY[L, p, j]) ", which is no " ITEM[p])
			if (p == "meta.replace") for (j = 1; j <= NI[L, p]; j++)
				if (!((L, ISTR[L, p, j]) in SH))
					refuse(at(L, p) "meta.replace names " ISTR[L, p, j] ", which this layer does not hold")
			continue
		}
		seen = 0
		for (k in ITEM) if (index(p, k "[") == 1 || index(p, k ".") == 1) seen = 1
		if (index(p, "meta.when.") == 1 || index(p, "meta.when[") == 1) seen = 1
		if (seen) continue
		refuse(at(L, p) p " is no key of the meta table")
	}
}

# The predicate defined at node p of L: its one kind into PKIND, at PKEY;
# refused by name where it holds no kind, two, or one over the wrong shape.
function define(L, p, name,   i, c, kind, n, j, bad) {
	n = 0; bad = SH[L, p] != "t"
	for (i = 1; i <= NN[L] && !bad; i++) {
		c = NODES[L, i]
		if (index(c, p ".") != 1) continue
		kind = substr(c, length(p) + 2)
		if (index(kind, ".") || index(kind, "[")) continue
		n++
		PKIND = kind; PKEY = c
		if ((kind == "env" || kind == "shell" || kind == "not") && SH[L, c] == "s" && TY[L, c] == "str") continue
		if ((kind == "all" || kind == "any") && SH[L, c] == "l") {
			for (j = 1; j <= NI[L, c]; j++) if (ITY[L, c, j] != "str") bad = 1
			continue
		}
		bad = 1
	}
	if (bad || n != 1) refuse(at(L, p) "the predicate " name " is not one of env, shell, not, all or any of its shape")
}

function defined(name) { return (B, "meta.predicates." name) in SH || (DF && (DF, "meta.predicates." name) in SH) }

# The table of predicates, of the base and of defaults, checked whole: every
# definition one kind of its shape, and every name a not, all or any gives
# defined.
function predicates(   s, L, i, p, name, j) {
	for (s = 1; s <= 2; s++) {
		L = s == 1 ? B : DF
		if (!L) continue
		for (i = 1; i <= NN[L]; i++) {
			p = NODES[L, i]
			if (substr(p, 1, 16) != "meta.predicates.") continue
			name = substr(p, 17)
			if (index(name, ".") || index(name, "[")) continue
			define(L, p, name)
			if (PKIND == "not" && !defined(STR[L, PKEY]))
				refuse(at(L, p) "no predicate " STR[L, PKEY] " is defined under meta.predicates")
			if (PKIND == "all" || PKIND == "any") for (j = 1; j <= NI[L, PKEY]; j++)
				if (!defined(ISTR[L, PKEY, j]))
					refuse(at(L, p) "no predicate " ISTR[L, PKEY, j] " is defined under meta.predicates")
		}
	}
	loops()
}

# Then a loop, walking each definition in turn, its names depth first in the
# order written, answering no variable and running nothing: the first name
# met again on the path that reached it is refused at the definition that
# named it.
function loops(   s, L, i, p, name) {
	for (s = 1; s <= 2; s++) {
		L = s == 1 ? B : DF
		if (!L) continue
		for (i = 1; i <= NN[L]; i++) {
			p = NODES[L, i]
			if (substr(p, 1, 16) != "meta.predicates.") continue
			name = substr(p, 17)
			if (index(name, ".") || index(name, "[")) continue
			PDEPTH = 0
			walk(name)
		}
	}
}

function walk(name,   i, from, pf, kind, dd, dk, j) {
	for (i = 1; i <= PDEPTH; i++) {
		if (CHAIN[i] != name) continue
		from = CHAIN[PDEPTH]
		pf = "meta.predicates." from
		refuse(at(((B, pf) in SH) ? B : DF, pf) "the predicate " name " is reached again from " from ", a loop")
	}
	definition(name)
	kind = DKIND; dd = DDOC; dk = DKEY
	CHAIN[++PDEPTH] = name
	if (kind == "not") walk(STR[dd, dk])
	else if (kind == "all" || kind == "any") for (j = 1; j <= NI[dd, dk]; j++) walk(ISTR[dd, dk, j])
	PDEPTH--
}

# --- finding the layers -----------------------------------------------------------

function adddoc(label, word) {
	ND++
	LAB[ND] = label
	WORD[ND] = word
	return ND
}

function newline(what) { refuse(what " a newline, which no line can name") }

# What a file is of this manifest, into LO ("isnt", "base", "layer") and
# LOWORD, refusing a file of its shape that cannot be a layer. A name of that
# shape holding a newline is refused as where says.
function layerof(file, label, where,   rest, word) {
	LO = "isnt"
	if (substr(file, 1, length(MNAME) + 1) != MNAME ".") return
	rest = substr(file, length(MNAME) + 2)
	if (length(rest) < length(MEXT) || substr(rest, length(rest) - length(MEXT) + 1) != MEXT) return
	if (nl(file)) newline(where)
	if (rest == MEXT) { LO = "base"; return }
	if (length(rest) <= length(MEXT) || substr(rest, length(rest) - length(MEXT)) != "." MEXT) return
	word = substr(rest, 1, length(rest) - length(MEXT) - 1)
	if (index(word, ".")) refuse(label " names two layer words; a layer is one")
	if (word == "base") refuse(label " names the word base, which is no layer")
	if (word !~ /^[a-z0-9_-]+$/) refuse(label " has a layer word outside a-z, 0-9, _ and -")
	LO = "layer"
	LOWORD = word
}

function two(a, b) { return a < b ? a " and " b : b " and " a }

# The canonical path of the file at label, or a refusal saying why it cannot
# be read.
function readable(label,   c) {
	c = canonical(label)
	if (c == "-notregular") refuse(label " is not a regular file")
	if (substr(c, 1, 1) != "/") refuse(label " cannot be read")
	if (nl(c)) newline(label ": its path holds")
	return c
}

# A file found as the part LO says: read once by its canonical path, its word
# carried by it alone. Answers the new document, or 0 where it was read before.
function found(label, beside,   word, c, d) {
	word = LO == "base" ? "" : LOWORD
	c = readable(label)
	if (c in SEEN) return 0
	if (word == "") refuse(two(LAB[B], label) " are both the base")
	if (word in HOLDER) refuse(two(LAB[HOLDER[word]], label) " carry the same layer word " word)
	if (word == "defaults" && !beside) refuse(label ": the defaults layer is read from beside the base only")
	d = adddoc(label, word)
	SEEN[c] = d
	HOLDER[word] = d
	return d
}

function includes(d,   k, j, e, dl, dir, pat, full, n, ent, i, lab, nd) {
	k = "meta.overlays.include"
	if (!((d, k) in SH)) return
	dl = dirof(LAB[d])
	for (j = 1; j <= NI[d, k]; j++) {
		e = ISTR[d, k, j]
		if (nl(e)) newline(at(d, k) k " names a path holding")
		if (e ~ /\/$/) {
			dir = joinlabel(dl, substr(e, 1, length(e) - 1))
			pat = "*"
		} else {
			full = joinlabel(dl, e)
			dir = dirof(full)
			pat = baseof(full)
		}
		split("", ent)
		n = listdir(dir, ent)
		if (n == -2 && e ~ /\/$/) refuse(at(d, k) k " names " e ", which is no directory")
		if (n == -1) refuse(at(d, k) k " names " e ", which cannot be listed")
		for (i = 1; i <= n; i++) {
			if (!glob(pat, ent[i])) continue
			if (nl(ent[i]) && e !~ /\/$/) newline(at(d, k) k " matches a path holding")
			lab = join2(dir, ent[i])
			layerof(ent[i], lab, at(d, k) k " matches a path holding")
			if (LO == "isnt") {
				if (e ~ /\/$/) continue
				refuse(at(d, k) k " matches " lab ", which is no layer of " LAB[B])
			}
			if ((nd = found(lab, 0)) > 0) QU[++QN] = nd
		}
	}
}

# --- the order and the untracked layers ---------------------------------------------

function got(t) { return t == "bool" ? "a boolean" : (t == "int" ? "an integer" : "a list") }

function readorder(   k, src, j, w, i) {
	k = "meta.overlays.order"
	src = ((B, k) in SH) ? B : ((DF && (DF, k) in SH) ? DF : 0)
	if (!src) { ORDN = split("* local secret private", ORD, " "); return }
	ORDN = 0
	for (j = 1; j <= NI[src, k]; j++) {
		w = ISTR[src, k, j]
		if (w == "defaults" || w == "base") refuse(at(src, k) k " names " w ", which stays below every overlay")
		for (i = 1; i <= ORDN; i++) if (ORD[i] == w) refuse(at(src, k) k " names " w " twice")
		ORD[++ORDN] = w
	}
}

function rank(d,   i, star, w) {
	w = WORD[d]
	star = -1
	for (i = 1; i <= ORDN; i++) {
		if (ORD[i] == w) return i - 1
		if (ORD[i] == "*") star = i - 1
	}
	if (star < 0) refuse(LAB[d] ": the layer " w " has no place in the order; add it or * to meta.overlays.order")
	return star
}

function untracked(   k, src, j, n, un, d, cmd) {
	k = "meta.overlays.untracked"
	src = ((B, k) in SH) ? B : ((DF && (DF, k) in SH) ? DF : 0)
	if (!src) n = split("local secret private", un, " ")
	else {
		n = NI[src, k]
		for (j = 1; j <= n; j++) un[j] = ISTR[src, k, j]
	}
	for (d = 1; d <= ND; d++) {
		if (WORD[d] == "") continue
		for (j = 1; j <= n; j++) if (un[j] == WORD[d]) break
		if (j > n) continue
		cmd = "cd -P -- " q(dpath(dirof(LAB[d]))) " && git ls-files --error-unmatch -- " \
			q(baseof(LAB[d])) " >/dev/null 2>&1"
		if (system(cmd) == 0)
			refuse(LAB[d] ": the layer " WORD[d] " is tracked by git and must not be; git rm --cached " LAB[d])
	}
}

# --- the predicates -----------------------------------------------------------------

function truthy(v,   l) {
	l = tolower(v)
	if (l == "1" || l == "true" || l == "on") return "1"
	if (l == "" || l == "0" || l == "false" || l == "no" || l == "off") return "0"
	return ""
}

function prefuse(msg) { refuse(at(PD, "meta.when") msg) }

# The definition of name, from the base and else from defaults, into DKIND,
# DDOC and DKEY, from the table checked whole when it was read; 0 where
# neither defines it.
function definition(name,   p, L) {
	p = "meta.predicates." name
	L = ((B, p) in SH) ? B : ((DF && (DF, p) in SH) ? DF : 0)
	if (!L) return 0
	define(L, p, name)
	DKIND = PKIND; DDOC = L; DKEY = PKEY
	return 1
}

function holds(name, d) {
	PD = d
	PDEPTH = 0
	return answer(name)
}

function answer(name,   i, kind, dd, dk, v, set, t, r, cmd, st, j, b) {
	for (i = 1; i <= PDEPTH; i++)
		if (CHAIN[i] == name) prefuse("the predicate " name " is reached again from " CHAIN[PDEPTH] ", a loop")
	if (!definition(name)) prefuse("no predicate " name " is defined under meta.predicates")
	kind = DKIND; dd = DDOC; dk = DKEY
	CHAIN[++PDEPTH] = name
	if (kind == "env") {
		v = STR[dd, dk]
		if (v == "LC_ALL") { set = LCSET == "set"; t = LCVAL }
		else { set = (v in ENVIRON); t = ENVIRON[v] }
		if (!set) r = 0
		else {
			r = truthy(t)
			if (r == "") prefuse(v " is " canon(t) ", which is neither true nor false")
			r += 0
		}
	} else if (kind == "shell") {
		# The LC_ALL of the caller is given back, and /bin/sh runs the command
		# whatever sh the path finds. The shell that waits on it says its
		# status, as system() says none alike in every awk, and what it says of
		# its own goes nowhere, while the error stream of the command goes to the
		# caller: past 128 a shell says a signal ended it, which is no exit.
		v = STR[dd, dk]
		cmd = (LCSET == "set" ? "LC_ALL=" q(LCVAL) "; export LC_ALL; " : "unset LC_ALL; ") \
			"/bin/sh -c " q("cd -- \"$1\" || { echo /125; exit; }; (exec /bin/sh -c \"$2\" >/dev/null 2>&3 3>&-); echo \"/$?\"") \
			" sh " q(dirof(CAN[B])) " " q(v) " 3>&2 2>/dev/null"
		st = output(cmd)
		if (st !~ /^\/[0-9]+$/) refuse(dirof(CAN[B]) ": no shell could be run there")
		st = substr(st, 2)
		if (st == "0") r = 1
		else if (st == "1") r = 0
		else if (st + 0 > 128) prefuse("`" v "` gave no exit")
		else prefuse("`" v "` exited " st ", which is neither 0 nor 1")
	} else if (kind == "not") {
		r = !answer(STR[dd, dk])
	} else {
		r = kind == "all"
		for (j = 1; j <= NI[dd, dk]; j++) {
			b = answer(ISTR[dd, dk, j])
			if (kind == "all") r = r && b
			else r = r || b
		}
	}
	PDEPTH--
	return r
}

# --- the merge ----------------------------------------------------------------------

# A shape by its name, a list told apart by n, how many items it holds.
function shapename(s, n) {
	return s == "t" ? "a table" : s == "s" ? "a scalar" : s == "a" ? "an array of tables" : \
		n == 0 ? "an empty list" : "a list of values"
}

# One shape over another that nothing replaces, at the key of the higher layer.
function clash(d, p, sh, n, lo, m) {
	refuse(LAB[d] ":" LN[d, p] ": " shapename(sh, n) " over " shapename(lo, m) \
		" does not merge; name the key in meta.replace to replace it")
}

function kindof(s) { return s == "a" ? "l" : s }

function drop(u,   p, gone, n, i) {
	n = 0
	for (p in MSH) if (p == u || index(p, u ".") == 1 || index(p, u "[") == 1) gone[++n] = p
	for (i = 1; i <= n; i++) { delete MSH[gone[i]]; delete MNI[gone[i]] }
}

# A path of layer L as it lies in the merged view, each array of tables
# offset by the elements the layers below gave it.
function translate(p,   out, lp, rest, i, j, lst, idx) {
	out = ""; lp = ""; rest = p
	while ((i = index(rest, "[")) > 0) {
		j = index(rest, "]")
		lst = lp substr(rest, 1, i - 1)
		idx = substr(rest, i + 1, j - i - 1) + ((lst in OFF) ? OFF[lst] : 0)
		out = out substr(rest, 1, i - 1) "[" idx "]"
		lp = lp substr(rest, 1, j)
		rest = substr(rest, j + 1)
	}
	return out rest
}

function merge(s,   d, k, j, u, t, held, i, p, tp, sh, lo) {
	d = STACK[s]
	k = "meta.unset"
	if ((d, k) in SH) for (j = 1; j <= NI[d, k]; j++) {
		if (ITY[d, k, j] != "str") continue
		u = ISTR[d, k, j]
		held = 0
		for (t = 1; t < s; t++) if ((STACK[t], u) in SH) held = 1
		if (!held) refuse(at(d, k) "meta.unset names " u ", which no lower layer holds")
		drop(u)
	}
	k = "meta.replace"
	if ((d, k) in SH) for (j = 1; j <= NI[d, k]; j++) if (ITY[d, k, j] == "str") drop(ISTR[d, k, j])
	split("", OFF)
	for (i = 1; i <= NN[d]; i++) {
		p = NODES[d, i]
		if (ismeta(p)) continue
		tp = translate(p)
		sh = SH[d, p]
		if (!(tp in MSH)) {
			MSH[tp] = sh; MLN[tp] = LN[d, p]; MLAB[tp] = LAB[d]
			if (sh == "s") MVAL[tp] = VAL[d, p]
			if (sh == "l") for (j = 1; j <= NI[d, p]; j++) MIT[tp, j] = IT[d, p, j]
			if (sh == "l" || sh == "a") MNI[tp] = NI[d, p]
			if (sh == "a") OFF[p] = 0
			continue
		}
		lo = MSH[tp]
		if (kindof(lo) != kindof(sh))
			clash(d, p, sh, NI[d, p], lo, MNI[tp])
		if (sh == "t") continue
		if (sh == "s") { MVAL[tp] = VAL[d, p]; MLN[tp] = LN[d, p]; MLAB[tp] = LAB[d]; continue }
		if (lo == "l" && sh == "l") {
			for (j = 1; j <= NI[d, p]; j++) MIT[tp, MNI[tp] + j] = IT[d, p, j]
			MNI[tp] += NI[d, p]
		} else if (lo == "a" && sh == "a") {
			OFF[p] = MNI[tp]
			MNI[tp] += NI[d, p]
		} else if (lo == "l" && MNI[tp] == 0) {
			MSH[tp] = "a"; MNI[tp] = NI[d, p]; OFF[p] = 0
		} else if (!(lo == "a" && NI[d, p] == 0)) {
			clash(d, p, sh, NI[d, p], lo, MNI[tp])
		}
		MLN[tp] = LN[d, p]; MLAB[tp] = LAB[d]
	}
}

# --- the whole manifest ---------------------------------------------------------------

function main(   i, n, ent, lab, d, qi, m, key, keys, sorted, j, t, p, line, lines) {
	lists()
	BDIR = dirof(BASE)
	lab = baseof(BASE)
	if (nl(BASE)) newline((nl(lab) ? "the base" : lab) ": its path holds")
	for (i = length(lab); i > 0 && substr(lab, i, 1) != "."; i--);
	if (i <= 1 || i == length(lab)) refuse(lab ": a manifest is name.ext")
	MNAME = substr(lab, 1, i - 1)
	MEXT = substr(lab, i + 1)
	ND = 0; QN = 0; DF = 0
	B = adddoc(lab, "")
	CAN[B] = readable(lab)
	SEEN[CAN[B]] = B
	parse(B)
	defect(B, "base")
	split("", ent)
	n = listdir("", ent)
	if (n < 0) refuse(lab ": its directory cannot be listed")
	for (i = 1; i <= n; i++) {
		layerof(ent[i], ent[i], lab ": its directory holds a layer whose name holds")
		if (LO == "layer" && LOWORD == "defaults") {
			DF = found(ent[i], 1)
			parse(DF)
			defect(DF, "defaults")
		}
	}
	predicates()
	QU[++QN] = B
	if (DF) QU[++QN] = DF
	for (i = 1; i <= n; i++) {
		layerof(ent[i], ent[i], lab ": its directory holds a layer whose name holds")
		if (LO == "layer" && LOWORD != "defaults" && (d = found(ent[i], 1)) > 0) QU[++QN] = d
	}
	readorder()
	for (qi = 1; qi <= QN; qi++) {
		d = QU[qi]
		if (d != B && d != DF) {
			parse(d)
			defect(d, "layer")
			if ((d, "meta.when") in SH && !holds(STR[d, "meta.when"], d)) continue
		}
		READ[d] = 1
		includes(d)
	}
	untracked()
	m = 0
	for (d = 1; d <= ND; d++) {
		if (d == B || d == DF) continue
		key = sprintf("%06d", rank(d)) " " WORD[d]
		if (!READ[d]) continue
		for (j = m; j > 0 && keys[j] > key; j--) { keys[j + 1] = keys[j]; sorted[j + 1] = sorted[j] }
		keys[j + 1] = key; sorted[j + 1] = d
		m++
	}
	SN = 0
	if (DF) STACK[++SN] = DF
	STACK[++SN] = B
	for (j = 1; j <= m; j++) STACK[++SN] = sorted[j]
	for (i = 1; i <= SN; i++) merge(i)
	n = 0
	for (p in MSH) {
		if (MSH[p] == "s") line = p " = " MVAL[p]
		else if (MSH[p] == "l") {
			line = p " = ["
			for (j = 1; j <= MNI[p]; j++) line = line (j > 1 ? ", " : "") MIT[p, j]
			line = line "]"
		}
		else continue
		if (VERB == "get" && p != WANT) continue
		for (j = n; j > 0 && lines[j] > line; j--) lines[j + 1] = lines[j]
		lines[j + 1] = line
		n++
	}
	if (VERB == "get" && n == 0) exit 1
	for (i = 1; i <= n; i++) print lines[i]
}
'
