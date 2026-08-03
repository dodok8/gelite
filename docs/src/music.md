# Music catalog

This Chinook-style example demonstrates several consecutive single links and an
explicit playlist association object.

```text
{{#include ../../examples/music.geli}}
```

## Create data

```sh
cargo run -p gelite-cli -- schema apply examples/music.geli --database music.db
cargo run -p gelite-cli -- repl --database music.db
```

Insert an artist, album, two tracks, and a playlist in that order. Keep every
generated ID used by the next insert:

```text
insert Artist { name := "Coco Sawatari", country := "Japan" }
```

```text
insert Album {
  title := "Midnight Testimony",
  release_year := 2026,
  artist := "<coco-id>"
}
```

```text
insert Track {
  title := "First Deduction",
  track_no := 1,
  duration_seconds := 214,
  genre := "mystery pop",
  album := "<album-id>"
}
```

```text
insert Track {
  title := "After the Bell",
  track_no := 2,
  duration_seconds := 188,
  genre := "mystery pop",
  album := "<album-id>"
}
```

```text
insert Playlist { name := "Sheri's Case Notes" }
```

```text
insert PlaylistTrack {
  position := 1,
  playlist := "<playlist-id>",
  track := "<first-track-id>"
}
```

## Query through the catalog

```text
select PlaylistTrack {
  position,
  track: {
    label := concat(.title, " / ", .album.title),
    duration_seconds,
    genre,
    album: {
      title,
      artist: {
        name,
        country
      }
    }
  },
  playlist: {
    name
  }
}
filter .playlist.name = "Sheri's Case Notes"
  and .track.duration_seconds >= 180
order by .position asc
limit 20
offset 0
```

The filter and ordering traverse stored single links. Runtime nested result
reconstruction remains deferred.
