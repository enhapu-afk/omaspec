#!/usr/bin/env bash
# The gallery's parcel growing over six commits, in a repository of its own
# at $1: the history `omaspec play parcel.puml --history` plays for the
# README (update.sh). Each commit a day after the one before.
set -euo pipefail
repo=$1
rm -rf "$repo"; mkdir -p "$repo"; cd "$repo"
git init -q
git config user.name "Omaspec"; git config user.email "omaspec@example.org"
day=10
commit() {
  cat > parcel.puml
  git add parcel.puml
  GIT_AUTHOR_DATE="2026-11-${day}T10:00:00" GIT_COMMITTER_DATE="2026-11-${day}T10:00:00" \
    git commit -q -m "$1"
  day=$((day + 1))
}
commit "feat: an order is packed" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Packed : pay
@enduml
P
commit "feat: an order can be cancelled" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Cancelled : cancel
Ordered --> Packed : pay
Cancelled --> [*]
@enduml
P
commit "feat: the parcel goes on its way" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Cancelled : cancel
Ordered --> Packed : pay
Packed --> Underway : handed over
Cancelled --> [*]
@enduml
P
commit "feat: sorted, then on the road" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Cancelled : cancel
Ordered --> Packed : pay
state Underway {
  [*] --> Sorting
  Sorting --> OnTheRoad : loaded
}
Packed --> Underway : handed over
Cancelled --> [*]
@enduml
P
commit "feat: delivered at the door" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Cancelled : cancel
Ordered --> Packed : pay
state Underway {
  [*] --> Sorting
  Sorting --> OnTheRoad : loaded
}
Packed --> Underway : handed over
Underway --> Delivered : arrives
Delivered --> [*]
Cancelled --> [*]
@enduml
P
commit "feat: nobody home, so it comes again" <<'P'
@startuml
title A parcel
[*] --> Ordered
Ordered --> Cancelled : cancel
Ordered --> Packed : pay
state Underway {
  [*] --> Sorting
  Sorting --> OnTheRoad : loaded
}
Packed --> Underway : handed over
state doorstep <<choice>>
Underway --> doorstep : arrives
doorstep --> Delivered : [someone home]
doorstep --> Underway : [nobody home]
Delivered --> [*]
Cancelled --> [*]
@enduml
P
