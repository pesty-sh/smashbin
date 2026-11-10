# Tutorial diagram

smashbin is two crates. The binary starts the library. The library has two doors. The ordinary door handles one job. The batch door decodes once and writes every size.

## Components

`main` is the only code in the `smashbin` binary. `start` is the only public function in `smashbin-core`. Everything else stays inside the library.

```mermaid
flowchart TB
    subgraph smashbin ["component smashbin"]
        main["main"]
    end
    subgraph core ["component smashbin-core"]
        start["start"]
        smashOne["smash_one"]
        batch["batch"]
        steps["read, sniff, strip, resize, lossless, lossy, convert, density, write"]
        config["config"]
    end
    main -->|"calls"| start
    start -->|"photo.jpg"| smashOne
    start -->|"the word batch"| batch
    smashOne --> steps
    batch --> steps
    smashOne -->|"smasher.json"| config
    batch -->|"gallery.yaml"| config
```

`smash_one` and `batch` both use the step files. They do not call each other. A `smasher.json` beside a photo reaches `config` through `smash_one`. `gallery.yaml` reaches `config` through `batch`.

## Ordinary command

One file goes through the steps once. A smasher file repeats that whole trip, including the read, once per job.

```mermaid
sequenceDiagram
    actor You
    participant Bin as smashbin
    participant Core as smashbin-core
    participant Disk
    You->>Bin: smashbin photo.jpg
    Bin->>Core: start
    Core->>Disk: read the file
    Core->>Core: sniff, strip, resize, shrink, write
    Core-->>You: one image, exit 0
```

## Batch command

The job list is read first. The image is read once and decoded once. Each size is fitted from that picture, and the JPEGs are encoded together. stderr then prints the clock.

```mermaid
sequenceDiagram
    actor You
    participant Core as smashbin-core
    participant Disk
    You->>Core: smashbin batch --jobs gallery.yaml --out variants photo.jpg
    Core->>Disk: read the job list
    Core->>Disk: read the image once
    Core->>Core: refuse a huge pixel count
    Core->>Core: decode once and turn a phone photo upright
    par one thread per size
        Core->>Core: cover or contain, then encode JPEG
    end
    Core->>Disk: write each named file
    Core-->>You: 23 files in the measured seconds
```
