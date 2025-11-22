use eyre::Result;
use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};

const CHUNK_SIZE_BYTES: usize = 256 * 1024 * 1024; // 256MB chunks
const BUF_SIZE: usize = 1024 * 1024; // 1MB buffer

/// External sort for a file of 32-byte hashes
pub fn external_sort(file_path: &str) -> Result<()> {
    let file_size = std::fs::metadata(file_path)?.len() as usize;
    let num_hashes = file_size / 32;

    if num_hashes == 0 {
        return Ok(());
    }

    // If file fits in memory, do in-place sort
    if file_size <= CHUNK_SIZE_BYTES {
        in_place_sort(file_path)?;
        return Ok(());
    }

    // Otherwise, chunk-based external sort
    let chunks_per_pass = CHUNK_SIZE_BYTES / 32;
    let num_chunks = num_hashes.div_ceil(chunks_per_pass);

    // Sort each chunk
    let mut chunk_files = Vec::new();
    for chunk_idx in 0..num_chunks {
        let start_hash = chunk_idx * chunks_per_pass;
        let end_hash = ((chunk_idx + 1) * chunks_per_pass).min(num_hashes);
        let chunk_size = end_hash - start_hash;

        let mut hashes = vec![[0u8; 32]; chunk_size];
        let mut file = File::open(file_path)?;
        file.seek(SeekFrom::Start((start_hash * 32) as u64))?;

        for hash in &mut hashes {
            std::io::Read::read_exact(&mut file, hash)?;
        }

        hashes.sort_unstable();

        let chunk_path = format!("{}.chunk{}", file_path, chunk_idx);
        let mut writer = BufWriter::with_capacity(BUF_SIZE, File::create(&chunk_path)?);
        for hash in hashes {
            writer.write_all(&hash)?;
        }
        writer.flush()?;

        chunk_files.push(chunk_path);
    }

    // K-way merge
    k_way_merge_hashes(&chunk_files, file_path)?;

    // Clean up chunks
    for chunk in chunk_files {
        std::fs::remove_file(chunk)?;
    }

    Ok(())
}

/// In-place sort for files that fit in memory
fn in_place_sort(file_path: &str) -> Result<()> {
    let mut file = File::open(file_path)?;
    let file_size = file.metadata()?.len() as usize;
    let num_hashes = file_size / 32;

    let mut hashes = vec![[0u8; 32]; num_hashes];
    for hash in &mut hashes {
        std::io::Read::read_exact(&mut file, hash)?;
    }

    hashes.sort_unstable();

    drop(file);

    let mut writer = BufWriter::with_capacity(BUF_SIZE, File::create(file_path)?);
    for hash in hashes {
        writer.write_all(&hash)?;
    }
    writer.flush()?;

    Ok(())
}

/// K-way merge of sorted hash files
fn k_way_merge_hashes(input_files: &[String], output_path: &str) -> Result<()> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    struct HashReader {
        reader: BufReader<File>,
        current: Option<[u8; 32]>,
    }

    impl HashReader {
        fn new(path: &str) -> Result<Self> {
            let mut reader = BufReader::with_capacity(BUF_SIZE, File::open(path)?);
            let current = Self::read_hash(&mut reader)?;
            Ok(Self { reader, current })
        }

        fn read_hash(reader: &mut BufReader<File>) -> Result<Option<[u8; 32]>> {
            let mut hash = [0u8; 32];
            match std::io::Read::read_exact(reader, &mut hash) {
                Ok(()) => Ok(Some(hash)),
                Err(_) => Ok(None),
            }
        }

        fn advance(&mut self) -> Result<()> {
            self.current = Self::read_hash(&mut self.reader)?;
            Ok(())
        }
    }

    #[derive(Eq, PartialEq)]
    struct HeapItem {
        hash: [u8; 32],
        reader_idx: usize,
    }

    impl Ord for HeapItem {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.hash.cmp(&other.hash)
        }
    }

    impl PartialOrd for HeapItem {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut readers: Vec<HashReader> = input_files
        .iter()
        .map(|path| HashReader::new(path))
        .collect::<Result<Vec<_>>>()?;

    let mut heap: BinaryHeap<Reverse<HeapItem>> = BinaryHeap::new();

    // Initialize heap
    for (idx, reader) in readers.iter().enumerate() {
        if let Some(hash) = reader.current {
            heap.push(Reverse(HeapItem {
                hash,
                reader_idx: idx,
            }));
        }
    }

    let mut output = BufWriter::with_capacity(BUF_SIZE, File::create(output_path)?);

    while let Some(Reverse(item)) = heap.pop() {
        output.write_all(&item.hash)?;

        // Advance the reader that produced this item
        readers[item.reader_idx].advance()?;
        if let Some(hash) = readers[item.reader_idx].current {
            heap.push(Reverse(HeapItem {
                hash,
                reader_idx: item.reader_idx,
            }));
        }
    }

    output.flush()?;
    Ok(())
}
