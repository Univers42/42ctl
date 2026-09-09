#!/usr/bin/env bash
# s33 — objects too large for the vault to carry.
#
# The vault refuses anything above one envelope, and the answer is not a bigger limit: a
# call that seals the whole payload in memory cannot carry a volume however the limit is
# set. Chunks go to object storage, and the vault keeps only the key and the chunk list.
#
# The assertions here are ordered so that each destructive one is preceded by the control
# that gives it meaning. "A broken store fails the read" says nothing unless the same read
# succeeded a moment earlier from the same fixture, so that success is asserted first and
# only then is the store broken. Two of this battery's worst bugs were assertions that
# passed over a haystack containing nothing, and a tamper check is the same shape.
#
# What is still red is red for two different reasons, and they are not interchangeable.
# Garbage collection is unbuilt: content-addressed chunks are never overwritten, so an
# edited file leaves its predecessors behind and only collection removes them. Deduplication
# BETWEEN members of one environment is blocked on a decision rather than on effort — it
# needs identical plaintext to produce identical ciphertext, which needs a convergent-chunk
# primitive in vault42-core, whose AEAD is private. Deduplication within one identity does
# work, and is asserted below, because a content-addressed name is the same for the same
# bytes whoever asks.

source "$(dirname "${BASH_SOURCE[0]}")/../lib/harness.sh"
source "$QA_LIB_DIR/server.sh"
source "$QA_LIB_DIR/fixtures.sh"

spec_begin "s33-large-objects"
qa_require_docker_stack
qa_require_cmd curl
trap 'qa_server_cleanup; [ "${QA_KEEP_SERVER:-0}" = "1" ] || { qa_authority_down; qa_s3_down; }' EXIT
qa_reclaim_workspace
qa_build_client >/dev/null 2>&1
assert_green "vault42-server is listening" -- qa_server_up
assert_green "an object store is available for chunks" -- qa_s3_up
[ "$_qa_regression" -eq 0 ] || {
	spec_end
	exit 1
}

W="$QA_RESULTS/s33"
rm -rf "$W"
mkdir -p "$W/src" "$W/restore" "$W/restore-edited"
qa_actor_reset chunker
CANARY="QA42-S33-PLAINTEXT-CANARY-0001"
fixture_project_marker "$W/src" s33-object '"*"'
fixture_varied_file "$W/src/volume.bin" 8 "$CANARY"

# ── the refusal, while nothing is configured ─────────────────────────────────
# A refusal is the right answer when there is nowhere for the bytes to go. A push that
# reports success while carrying nothing is worse than one that fails, because the loss is
# discovered at the restore, which is the one moment nobody can afford to discover it.
assert_green "an oversized file is refused while no object store is configured" \
	-- bash -c '! qa_actor chunker "$1" "push" >/dev/null 2>&1' _ "$W/src"
assert_green "the refusal names the setting that would make it work" \
	-- bash -c 'qa_actor chunker "$1" "push" 2>&1 | grep -q -- "--blobstore"' _ "$W/src"

# ── configuring where large objects go ───────────────────────────────────────
assert_green "the CLI accepts an object store endpoint setting" \
	-- qa_actor chunker "$W/src" "config endpoint --blobstore $(qa_s3_internal) --bucket $QA_S3_BUCKET"
assert_green "the resolved configuration reports the object store" \
	-- bash -c 'qa_actor chunker "$1" "config show" 2>&1 | grep -qi "object-store"' _ "$W/src"
assert_green "the saved configuration holds no credential" \
	-- bash -c '! grep -qiE "secret|password|access[_-]?key" "$QA_RESULTS/actors/chunker/config.json"'

# ── the transfer ─────────────────────────────────────────────────────────────
# NOTE: no --project on push. Passing it replaces the marker's configured scan patterns
# with the defaults, so the payload would not be scanned at all and push would exit 0
# having transferred nothing — which is exactly how the first version of this passed.
qa_s3_names >"$W/names.before"
qa_dump_server_db "$W/db.before"
assert_green "a file above the transport ceiling pushes via chunks" \
	-- bash -c 'out=$(qa_actor chunker "$1" "push" 2>&1) || { printf "%s\n" "$out"; exit 1; }
		grep -qE "pushed [1-9]" <<<"$out"' _ "$W/src"
qa_s3_names >"$W/names.after"
comm -13 "$W/names.before" "$W/names.after" >"$W/names.new"

assert_green "the bytes landed in the object store as more than one chunk" \
	-- bash -c '[ "$(grep -c . "$1")" -ge 2 ]' _ "$W/names.new"

# The whole economic argument is that a volume does not land on the fly volume. Measured as
# GROWTH rather than as absolute size, so the assertion says something about this push
# rather than about whatever the other specs left in the database.
assert_green "the vault took the chunk list and not the eight megabytes" \
	-- bash -c 'qa_dump_server_db "$2" || exit 1
		grew=$(($(stat -c %s "$2") - $(stat -c %s "$1")))
		[ "$grew" -lt 1048576 ] || { printf "the vault grew by %s bytes\n" "$grew"; exit 1; }' \
	_ "$W/db.before" "$W/db.after"

assert_green "the file pulls back byte-identical" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-object --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/src" "$W/restore"

# ── opacity, each search over a haystack proved to be real ───────────────────
assert_green "the object store dump contains something to search" -- qa_s3_dump "$W/s3.dump"
assert_zero_knowledge "the object store holds no plaintext" \
	"$CANARY" "chunk" "$W/s3.dump"
assert_zero_knowledge "the object store never learns the file name" \
	"volume.bin" "chunk" "$W/s3.dump"
assert_zero_knowledge "the object store never learns which project a chunk belongs to" \
	"s33-object" "chunk" "$W/s3.dump"
assert_zero_knowledge "the vault never learns the file name either" \
	"volume.bin" "s33-object" "$W/db.after"

# ── resume, which is what makes a gigabyte survivable ────────────────────────
assert_green "a second push of unchanged bytes uploads nothing" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1 || exit 1
		qa_s3_names >"$2/names.resume"
		cmp -s "$2/names.after" "$2/names.resume"' _ "$W/src" "$W"

# ── the trap the resume check sets, and the assertion that springs it ────────
# Skipping a chunk the store already holds is only safe because a chunk is named by its
# content. Name it by its position instead and the skip becomes silent corruption: the
# changed bytes never upload, the list still validates, and the read returns the previous
# version looking entirely healthy. This ran red against exactly that.
printf 'EDITED-BY-S33' | dd of="$W/src/volume.bin" bs=1 seek=64 conv=notrunc status=none 2>/dev/null
assert_green "an edit costs only the chunks it actually touched" \
	-- bash -c 'before=$(qa_s3_count)
		qa_actor chunker "$1" "push" >/dev/null 2>&1 || exit 1
		after=$(qa_s3_count)
		[ "$((after - before))" -eq 1 ] || { printf "the store grew by %s objects, not 1\n" "$((after - before))"; exit 1; }' \
	_ "$W/src"
assert_green "the edited file pulls back as edited, not as the version before it" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-object --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/src" "$W/restore-edited"

# ── the ceiling itself, measured rather than read off the source ─────────────
# One byte either side of the chunk size. Below it the file must travel through the vault
# alone and leave the object store untouched; above it the store must gain objects. This is
# what pins the real transport ceiling behaviourally: the client guard read 64 MiB for most
# of this crate's life, sixteen times what the wire accepts.
#
# The fixtures are written HERE and not inside an assertion. The fixture helpers are shell
# functions and are not exported, so calling one inside `bash -c` fails with "command not
# found" and leaves no file — which the first version of this did, and the assertion passed
# because an object store that gained nothing is exactly what a file that never existed
# produces. The size is asserted before anything is pushed for the same reason.
mkdir -p "$W/edge" "$W/edge-restore"
fixture_project_marker "$W/edge" s33-edge '"*"'
fixture_sized_file "$W/edge/volume.bin" 4128768
assert_green "the fixture at the ceiling is exactly one chunk long" \
	-- bash -c '[ "$(stat -c %s "$1/volume.bin")" -eq 4128768 ]' _ "$W/edge"
EDGE_BEFORE="$(qa_s3_count)"
assert_green "a file of exactly one chunk does not touch the object store" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1 || exit 1
		[ "$(qa_s3_count)" -eq "$2" ] || { printf "a file at the ceiling was chunked\n"; exit 1; }' \
	_ "$W/edge" "$EDGE_BEFORE"

fixture_sized_file "$W/edge/volume.bin" 4128769
assert_green "the fixture one byte over the ceiling is one byte longer" \
	-- bash -c '[ "$(stat -c %s "$1/volume.bin")" -eq 4128769 ]' _ "$W/edge"
EDGE_BEFORE="$(qa_s3_count)"
assert_green "one byte more goes to the object store" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1 || exit 1
		[ "$(qa_s3_count)" -gt "$2" ] || { printf "a file over the ceiling was not chunked\n"; exit 1; }' \
	_ "$W/edge" "$EDGE_BEFORE"
assert_green "the file one byte over the ceiling pulls back byte-identical" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-edge --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/edge" "$W/edge-restore"

# ── a push that finds part of its work already done ──────────────────────────
# The state an interrupted upload leaves behind: some chunks in the store, the rest missing.
# Resuming must send only what is absent and still reproduce the file exactly. One chunk is
# removed deliberately rather than by racing a real interruption, so the case is
# reproducible instead of timing-dependent.
#
# From an empty store, so the chunk that gets removed provably belongs to this file.
# Removing whichever object happened to sort first would take another project's chunk, and
# the assertion would then fail for a reason that has nothing to do with resuming.
qa_mc "mc rm --recursive --force qa/$QA_S3_BUCKET" >/dev/null 2>&1
mkdir -p "$W/resume" "$W/resume-restore"
fixture_project_marker "$W/resume" s33-resume '"*"'
fixture_varied_file "$W/resume/volume.bin" 12 "QA42-S33-RESUME-CANARY"
assert_green "the store is empty before the resume fixture is pushed" \
	-- bash -c '[ "$(qa_s3_count)" -eq 0 ]'
assert_green "the resume fixture pushes in full" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1' _ "$W/resume"
assert_green "a push whose chunks are partly missing restores exactly what it had" \
	-- bash -c 'full=$(qa_s3_count)
		qa_s3_names >"$2/resume.names.full"
		qa_s3_rm "$(sed -n 1p "$2/resume.names.full")"
		[ "$(qa_s3_count)" -lt "$full" ] || { printf "nothing was removed, so nothing was resumed\n"; exit 1; }
		qa_actor chunker "$1" "push" >/dev/null 2>&1 || exit 1
		qa_s3_names >"$2/resume.names.again"
		cmp -s "$2/resume.names.full" "$2/resume.names.again" ||
			{ diff "$2/resume.names.full" "$2/resume.names.again"; exit 1; }' _ "$W/resume" "$W"
assert_green "the resumed file pulls back byte-identical" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-resume --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/resume" "$W/resume-restore"

# ── a store that lies: substitution, then loss ───────────────────────────────
# From an empty store, so every object in it belongs to the file under test and
# "substitute one chunk for another" names a known chunk rather than whichever object
# happened to sort first.
qa_mc "mc rm --recursive --force qa/$QA_S3_BUCKET" >/dev/null 2>&1
mkdir -p "$W/tamper" "$W/tamper-restore"
fixture_project_marker "$W/tamper" s33-tamper '"*"'
fixture_varied_file "$W/tamper/volume.bin" 8 "QA42-S33-TAMPER-CANARY"
assert_green "the tamper fixture pushes" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1' _ "$W/tamper"
assert_green "the tamper fixture pulls back byte-identical BEFORE anything is broken" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-tamper --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/tamper" "$W/tamper-restore"
assert_green "a substituted chunk is refused rather than reassembled into wrong bytes" \
	-- bash -c 'a=$(qa_s3_names | sed -n 1p); b=$(qa_s3_names | sed -n 2p)
		[ -n "$a" ] && [ -n "$b" ] || { printf "fewer than two chunks, so nothing was substituted\n"; exit 1; }
		qa_s3_substitute "$a" "$b"
		rm -rf "$1"; mkdir -p "$1"
		! qa_actor chunker "$1" "pull --project s33-tamper --apply" >/dev/null 2>&1' _ "$W/tamper-swapped"
assert_green "a deleted chunk is reported rather than silently truncating" \
	-- bash -c 'qa_mc "mc rm --recursive --force qa/$QA_S3_BUCKET" >/dev/null 2>&1
		rm -rf "$1"; mkdir -p "$1"
		qa_actor chunker "$1" "pull --project s33-tamper --apply" >/dev/null 2>&1 && exit 1
		[ ! -f "$1/volume.bin" ] || { printf "a partial file was written\n"; exit 1; }' _ "$W/tamper-gone"

# ── garbage collection, which content addressing makes mandatory ─────────────
# Chunks are never overwritten, so every edit leaves its predecessors behind. Collection is
# also the one operation here that can destroy data silently, so each of its safety rails is
# asserted separately rather than folded into one "gc works" check.
#
# The store is rebuilt from empty and given TWO versions of one file, so the set of objects
# is known exactly: three chunks from the first push, one more from the edit. Every one of
# those four is referenced by some manifest version, and none of them may be collected.
qa_mc "mc rm --recursive --force qa/$QA_S3_BUCKET" >/dev/null 2>&1
mkdir -p "$W/gc" "$W/gc-restore"
fixture_project_marker "$W/gc" s33-gc '"*"'
fixture_varied_file "$W/gc/volume.bin" 8 "QA42-S33-GC-CANARY"
assert_green "the collection fixture pushes its first version" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1' _ "$W/gc"
printf 'EDITED-FOR-GC' | dd of="$W/gc/volume.bin" bs=1 seek=64 conv=notrunc status=none 2>/dev/null
assert_green "the collection fixture pushes a second version" \
	-- bash -c 'qa_actor chunker "$1" "push" >/dev/null 2>&1' _ "$W/gc"
qa_s3_names >"$W/gc.names.live"

# Garbage: a copy of a real chunk under a name no manifest will ever mention. Made by
# copying rather than by inventing bytes, so it is indistinguishable from a chunk left
# behind by an interrupted push — which is the thing collection actually exists to remove.
assert_green "an unreferenced chunk can be planted for collection to find" \
	-- bash -c 'first=$(sed -n 1p "$1"); prefix=${first%/*}
		[ -n "$first" ] || { printf "no chunks to copy from\n"; exit 1; }
		qa_s3_substitute "$first" "$prefix/0000000000000000000000000000000000000000000000000000000000000000"
		[ "$(qa_s3_count)" -eq "$(($(grep -c . "$1") + 1))" ]' _ "$W/gc.names.live"

# The rail that matters most in practice: an interrupted push has already uploaded chunks
# that no manifest names yet, and resuming it is what makes a gigabyte survivable. Collecting
# them because they look unreferenced is how a resumable upload becomes an unresumable one.
assert_green "collection leaves a young chunk alone even though nothing references it" \
	-- bash -c 'before=$(qa_s3_count)
		qa_actor chunker "$1" "vault gc --apply" >/dev/null 2>&1 || exit 1
		[ "$(qa_s3_count)" -eq "$before" ] || { printf "collection deleted a chunk inside its grace period\n"; exit 1; }' \
	_ "$W/gc"

assert_green "a dry run names what it would collect and removes nothing" \
	-- bash -c 'before=$(qa_s3_count)
		out=$(qa_actor chunker "$1" "vault gc --grace-hours 0" 2>&1) || { printf "%s\n" "$out"; exit 1; }
		grep -q "would remove" <<<"$out" || { printf "%s\n" "$out"; exit 1; }
		[ "$(qa_s3_count)" -eq "$before" ]' _ "$W/gc"

# The assertion most worth breaking on purpose: a wrong collection destroys history in a way
# nothing reports. Comparing the FULL NAME SET rather than a count is what makes this catch a
# collection that removed the right number of objects and the wrong ones.
assert_green "collection removes the unreferenced chunk and nothing an older version needs" \
	-- bash -c 'qa_actor chunker "$1" "vault gc --grace-hours 0 --apply" >/dev/null 2>&1 || exit 1
		qa_s3_names >"$2/gc.names.after"
		cmp -s "$2/gc.names.live" "$2/gc.names.after" ||
			{ printf "the surviving chunks are not the ones both versions reference\n"
			  diff "$2/gc.names.live" "$2/gc.names.after"; exit 1; }' _ "$W/gc" "$W"

assert_green "the file still pulls back byte-identical after collection" \
	-- bash -c 'qa_actor chunker "$2" "pull --project s33-gc --apply" >/dev/null 2>&1
		cmp -s "$1/volume.bin" "$2/volume.bin"' _ "$W/gc" "$W/gc-restore"

# An empty reference set and an unread one look identical from the deletion side, so the
# case where a wrong answer deletes everything is the case that must refuse to proceed.
qa_actor_reset newcomer
assert_green "collection refuses to run for an identity with no manifests at all" \
	-- bash -c 'qa_actor newcomer "$1" "config endpoint --blobstore $2 --bucket $3" >/dev/null 2>&1
		out=$(qa_actor newcomer "$1" "vault gc --grace-hours 0 --apply" 2>&1) && exit 1
		grep -qi "refus" <<<"$out"' _ "$W/gc" "$(qa_s3_internal)" "$QA_S3_BUCKET"

# ── still blocked on a decision, not on effort ───────────────────────────────
# A chunk name is a keyed hash under a key derived per identity, so two members of one
# environment store separate copies of identical bytes. Sharing them needs identical
# plaintext to seal to identical CIPHERTEXT, which needs a convergent-chunk primitive in
# vault42-core: its AEAD is private, and a second copy of the cipher in this crate is what
# the project's own rules forbid. The signature that would satisfy these is agreed —
# seal_chunk(scope_secret, domain, plaintext) -> {name, ciphertext} with a content-derived
# nonce — so these assert what it must do, not merely that it is missing.
assert_spec "two identities in one environment name a chunk of identical bytes identically" \
	-- bash -c 'false  # blocked: needs a convergent chunk primitive in vault42-core'
assert_spec "the same bytes under a different environment secret get a different name" \
	-- bash -c 'false  # blocked: same — this is what stops equality leaking across environments'
assert_spec "a chunk sealed under a name that is not the hash of its bytes is refused" \
	-- bash -c 'false  # blocked: same — the poisoned-dedup case a malicious writer creates'
assert_spec "an older version still restores after a newer one is pushed" \
	-- bash -c 'false  # not built: pull always resolves the latest manifest'

spec_end
