#!/usr/bin/env bash
# s14 — how much data the vault can actually hold.
#
# MEASURED CEILING for the DIRECT path: about 4 MiB per file. Neither vault42-server nor
# 42ctl raises tonic's default max_decoding_message_size, so a sealed envelope larger than
# 4,194,304 bytes is refused with OutOfRange. Envelope overhead measured at roughly 500
# bytes, so the usable plaintext ceiling is a little under 4 MiB.
#
# Above that the file is chunked to an object store, and the sizes below the ceiling and
# above it are asserted in that order deliberately: the refusal when NO store is configured
# is asserted first, because once one is configured that refusal can no longer be observed,
# and an operator with no object store is the case where silent loss would hurt most.
#
# Green assertions pin what works so it cannot silently shrink.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/fixtures.sh"
source "$QA_LIB_DIR/server.sh"

spec_begin "s14-large-payloads"
qa_require_docker_stack
trap qa_server_cleanup EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
[ "$_qa_regression" -eq 0 ] || { spec_end; exit 1; }

WORK="$QA_RESULTS/s14"; rm -rf "$WORK"; mkdir -p "$WORK"

# Push a single file of SIZE bytes and pull it back into a clean tree.
# Echoes nothing; returns non-zero if the round trip did not reproduce the bytes.
roundtrip_size() {
	local bytes="$1" id="s14-$1" src="$WORK/src-$1" dst="$WORK/dst-$1"
	rm -rf "$src" "$dst"; mkdir -p "$src" "$dst"
	fixture_project_marker "$src" "$id" '"*"'
	fixture_sized_file "$src/volume.bin" "$bytes"
	qa_actor bigalice "$src" "push" >/dev/null 2>&1 || return 1
	qa_actor bigalice "$dst" "pull --project $id --apply" >/dev/null 2>&1 || return 1
	cmp -s "$src/volume.bin" "$dst/volume.bin"
}

# ── what works today, pinned so it cannot regress ────────────────────────────
assert_green "a 64 KiB file round-trips byte-for-byte"  -- roundtrip_size 65536
assert_green "a 1 MiB file round-trips byte-for-byte"   -- roundtrip_size 1048576
assert_green "a 3 MiB file round-trips byte-for-byte"   -- roundtrip_size 3145728

# ── failing loudly is the one thing it already gets right ────────────────────
# An oversize file must not be silently skipped the way an unscanned secrets/ tree is.
# Loud refusal is recoverable; a success message covering a missing file is not.
OVER="$WORK/oversize"; rm -rf "$OVER"; mkdir -p "$OVER"
fixture_project_marker "$OVER" s14-oversize '"*"'
fixture_sized_file "$OVER/too-big.bin" 8388608
# A DEDICATED actor, reset here. The refusal only happens while no object store is
# configured, and the section below configures one for bigalice — whose state persists in
# qa/results/actors between runs, so the next battery found the store already set and the
# file was chunked instead of refused. A green that depends on what the previous RUN left
# behind is worse than an order-dependent one, because it survives a shuffle.
qa_actor_reset refuser
assert_green "an oversize file makes push fail rather than silently skip it" \
	-- bash -c '! qa_actor refuser "$1" "push" >/dev/null 2>&1' _ "$OVER"
# Stronger than "the message mentions a limit": it must name the size that was refused AND
# the ceiling it exceeded, because a refusal an operator cannot act on is a refusal they
# will work around. The wording moved when chunking landed; the requirement did not.
assert_green "the oversize refusal names both the file's size and the ceiling it exceeded" \
	-- bash -c 'out=$(qa_actor refuser "$1" "push" 2>&1)
		grep -q 8388608 <<<"$out" || { printf "%s\n" "$out"; exit 1; }
		grep -qiE "ceiling|limit|too large|OutOfRange" <<<"$out"' _ "$OVER"

# ── above the ceiling, once somewhere exists for the bytes to go ─────────────
# The same sizes that were refused above now round-trip, because the file is split and the
# chunks go to object storage while the vault keeps only the list. Note the deliberate
# ordering: everything that depends on NO store being configured was asserted first.
assert_green "an object store is available" -- qa_s3_up
assert_green "the large-payload actor can be pointed at an object store" \
	-- qa_actor bigalice "$WORK" "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET"
assert_green "a 4 MiB file round-trips"  -- roundtrip_size 4194304
assert_green "a 16 MiB file round-trips" -- roundtrip_size 16777216
assert_green "a 64 MiB file round-trips" -- roundtrip_size 67108864

# fixture_sized_file writes one byte repeated, so a chunked copy of it collapses to a single
# stored object however large the file is. That is a real and correct case, and it is also
# the case that exercises the least: this one gives every mebibyte its own filler byte, so
# sixteen distinct chunks have to come back in the right order to reproduce the file.
roundtrip_varied() {
	local mib="$1" id="s14-varied-$1" src="$WORK/varied-$1" dst="$WORK/varied-dst-$1"
	rm -rf "$src" "$dst"; mkdir -p "$src" "$dst"
	fixture_project_marker "$src" "$id" '"*"'
	fixture_varied_file "$src/volume.bin" "$mib" "QA42-S14-VARIED-CANARY"
	qa_actor bigalice "$src" "push" >/dev/null 2>&1 || return 1
	qa_actor bigalice "$dst" "pull --project $id --apply" >/dev/null 2>&1 || return 1
	cmp -s "$src/volume.bin" "$dst/volume.bin"
}
assert_green "a 64 MiB file whose every mebibyte differs round-trips" -- roundtrip_varied 64

# ── a compressed volume, which is the real shape of the request ──────────────
# Attached data arrives as an archive, so the archive itself must survive intact.
# Compression makes this worse, not better: a single flipped byte in a .tar.gz
# usually destroys the whole archive rather than one file inside it.
ARC="$WORK/archive"; rm -rf "$ARC"; mkdir -p "$ARC/payload/nested"
for i in 1 2 3 4 5; do fixture_sized_file "$ARC/payload/nested/file$i.dat" 120000; done
printf 'DB_PASSWORD=archive-secret\n' >"$ARC/payload/.env"
tar czf "$ARC/bundle.tar.gz" -C "$ARC" payload
rm -rf "$ARC/payload"
fixture_project_marker "$ARC" s14-archive '"*"'
assert_green "a compressed archive pushes" -- qa_actor bigalice "$ARC" "push"
mkdir -p "$WORK/archive-restore"
assert_green "a compressed archive pulls" \
	-- qa_actor bigalice "$WORK/archive-restore" "pull --project s14-archive --apply"
assert_bytes_equal "the archive is byte-identical after the round trip" \
	"$ARC/bundle.tar.gz" "$WORK/archive-restore/bundle.tar.gz"
assert_green "the restored archive still extracts cleanly" \
	-- tar tzf "$WORK/archive-restore/bundle.tar.gz"

spec_end
