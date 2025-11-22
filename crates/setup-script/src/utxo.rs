use eyre::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

const CHUNK_SIZE: usize = 5_000_000; // 5M entries per chunk
const BUF_SIZE: usize = 1024 * 1024; // 1MB buffer

pub struct CreateEntry {
    pub txid: [u8; 32],
    pub vout: u32,
    pub value: i64,
    pub script: Vec<u8>,
}

pub struct SpendEntry {
    pub txid: [u8; 32],
    pub vout: u32,
}

/// Hash a UTXO as: sha256(txid || vout || value || lock_script)
/// Note: Converts i64 value to u64 before hashing for consistency with lib
pub fn hash_utxo(
    outpoint_txid: &[u8; 32],
    outpoint_vout: u32,
    value: i64,
    lock_script: &[u8],
) -> [u8; 32] {
    let value_u64 = value.max(0) as u64; // Convert to u64, clamping negatives to 0
    let mut hasher = Sha256::new();
    hasher.update(outpoint_txid);
    hasher.update(outpoint_vout.to_le_bytes());
    hasher.update(value_u64.to_le_bytes());
    hasher.update(lock_script);
    hasher.finalize().into()
}

/// Sort creates file in chunks
pub fn sort_creates_in_chunks(file_path: &str) -> Result<Vec<String>> {
    let mut file = BufReader::with_capacity(BUF_SIZE, File::open(file_path)?);
    let mut chunk_files = Vec::new();
    let mut chunk_index = 0;

    loop {
        let mut chunk = Vec::with_capacity(CHUNK_SIZE);

        // Read chunk
        for _ in 0..CHUNK_SIZE {
            let mut entry_buf = [0u8; 48]; // 32 + 4 + 8 + 4
            if file.read_exact(&mut entry_buf).is_err() {
                break;
            }

            let mut txid = [0u8; 32];
            txid.copy_from_slice(&entry_buf[0..32]);
            let vout =
                u32::from_le_bytes([entry_buf[32], entry_buf[33], entry_buf[34], entry_buf[35]]);
            let value = i64::from_le_bytes([
                entry_buf[36],
                entry_buf[37],
                entry_buf[38],
                entry_buf[39],
                entry_buf[40],
                entry_buf[41],
                entry_buf[42],
                entry_buf[43],
            ]);
            let script_len =
                u32::from_le_bytes([entry_buf[44], entry_buf[45], entry_buf[46], entry_buf[47]])
                    as usize;

            let mut script = vec![0u8; script_len];
            file.read_exact(&mut script)?;

            chunk.push(CreateEntry {
                txid,
                vout,
                value,
                script,
            });
        }

        if chunk.is_empty() {
            break;
        }

        // Sort chunk by (txid, vout)
        chunk.sort_unstable_by(|a, b| (&a.txid, a.vout).cmp(&(&b.txid, b.vout)));

        // Write sorted chunk
        let chunk_path = format!("{}.create_chunk{}", file_path, chunk_index);
        let mut writer = BufWriter::with_capacity(BUF_SIZE, File::create(&chunk_path)?);
        for entry in chunk {
            writer.write_all(&entry.txid)?;
            writer.write_all(&entry.vout.to_le_bytes())?;
            writer.write_all(&entry.value.to_le_bytes())?;
            writer.write_all(&(entry.script.len() as u32).to_le_bytes())?;
            writer.write_all(&entry.script)?;
        }
        writer.flush()?;

        chunk_files.push(chunk_path);
        chunk_index += 1;
    }

    Ok(chunk_files)
}

/// Sort spends file in chunks
pub fn sort_spends_in_chunks(file_path: &str) -> Result<Vec<String>> {
    const SPEND_CHUNK_SIZE: usize = 10_000_000; // 10M spends per chunk (36 bytes each)

    let mut file = BufReader::with_capacity(BUF_SIZE, File::open(file_path)?);
    let mut chunk_files = Vec::new();
    let mut chunk_index = 0;

    loop {
        let mut chunk = Vec::with_capacity(SPEND_CHUNK_SIZE);

        // Read chunk - batch read for efficiency
        let mut batch_buf = vec![0u8; SPEND_CHUNK_SIZE * 36]; // txid (32) + vout (4)
        let bytes_read = file.read(&mut batch_buf)?;
        if bytes_read == 0 {
            break;
        }

        let num_entries = bytes_read / 36;
        for i in 0..num_entries {
            let offset = i * 36;
            let mut txid = [0u8; 32];
            txid.copy_from_slice(&batch_buf[offset..offset + 32]);
            let vout = u32::from_le_bytes([
                batch_buf[offset + 32],
                batch_buf[offset + 33],
                batch_buf[offset + 34],
                batch_buf[offset + 35],
            ]);
            chunk.push(SpendEntry { txid, vout });
        }

        if chunk.is_empty() {
            break;
        }

        // Sort chunk by (txid, vout)
        chunk.sort_unstable_by(|a, b| (&a.txid, a.vout).cmp(&(&b.txid, b.vout)));

        // Write sorted chunk - batch write
        let chunk_path = format!("{}.spend_chunk{}", file_path, chunk_index);
        let mut writer = BufWriter::with_capacity(BUF_SIZE, File::create(&chunk_path)?);
        let mut write_buf = Vec::with_capacity(chunk.len() * 36);
        for entry in chunk {
            write_buf.extend_from_slice(&entry.txid);
            write_buf.extend_from_slice(&entry.vout.to_le_bytes());
        }
        writer.write_all(&write_buf)?;
        writer.flush()?;

        chunk_files.push(chunk_path);
        chunk_index += 1;
    }

    Ok(chunk_files)
}

struct CreateReader {
    reader: BufReader<File>,
    current: Option<CreateEntry>,
}

impl CreateReader {
    fn new(path: &str) -> Result<Self> {
        let mut reader = BufReader::with_capacity(BUF_SIZE, File::open(path)?);
        let current = Self::read_entry(&mut reader)?;
        Ok(Self { reader, current })
    }

    fn read_entry(reader: &mut BufReader<File>) -> Result<Option<CreateEntry>> {
        let mut header = [0u8; 48];
        if reader.read_exact(&mut header).is_err() {
            return Ok(None);
        }

        let mut txid = [0u8; 32];
        txid.copy_from_slice(&header[0..32]);
        let vout = u32::from_le_bytes([header[32], header[33], header[34], header[35]]);
        let value = i64::from_le_bytes([
            header[36], header[37], header[38], header[39], header[40], header[41], header[42],
            header[43],
        ]);
        let script_len =
            u32::from_le_bytes([header[44], header[45], header[46], header[47]]) as usize;

        let mut script = vec![0u8; script_len];
        reader.read_exact(&mut script)?;

        Ok(Some(CreateEntry {
            txid,
            vout,
            value,
            script,
        }))
    }

    fn advance(&mut self) -> Result<()> {
        self.current = Self::read_entry(&mut self.reader)?;
        Ok(())
    }

    fn key(&self) -> Option<([u8; 32], u32)> {
        self.current.as_ref().map(|e| (e.txid, e.vout))
    }
}

struct SpendReader {
    reader: BufReader<File>,
    current: Option<SpendEntry>,
}

impl SpendReader {
    fn new(path: &str) -> Result<Self> {
        let mut reader = BufReader::with_capacity(BUF_SIZE, File::open(path)?);
        let current = Self::read_entry(&mut reader)?;
        Ok(Self { reader, current })
    }

    fn read_entry(reader: &mut BufReader<File>) -> Result<Option<SpendEntry>> {
        let mut buf = [0u8; 36];
        if reader.read_exact(&mut buf).is_err() {
            return Ok(None);
        }

        let mut txid = [0u8; 32];
        txid.copy_from_slice(&buf[0..32]);
        let vout = u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);

        Ok(Some(SpendEntry { txid, vout }))
    }

    fn advance(&mut self) -> Result<()> {
        self.current = Self::read_entry(&mut self.reader)?;
        Ok(())
    }

    fn key(&self) -> Option<([u8; 32], u32)> {
        self.current.as_ref().map(|e| (e.txid, e.vout))
    }
}

/// K-way merge of creates and spends, outputting UTXO hashes
pub fn merge_and_compute_utxos(
    create_chunks: &[String],
    spend_chunks: &[String],
    output_path: &str,
) -> Result<usize> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut create_readers: Vec<CreateReader> = create_chunks
        .iter()
        .map(|path| CreateReader::new(path))
        .collect::<Result<Vec<_>>>()?;

    let mut spend_readers: Vec<SpendReader> = spend_chunks
        .iter()
        .map(|path| SpendReader::new(path))
        .collect::<Result<Vec<_>>>()?;

    let mut output = BufWriter::with_capacity(BUF_SIZE, File::create(output_path)?);
    let mut utxo_count = 0;

    // Build initial heap for creates (min-heap by key)
    #[derive(Eq, PartialEq)]
    struct CreateHeapItem {
        key: ([u8; 32], u32),
        reader_idx: usize,
    }

    impl Ord for CreateHeapItem {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.key.cmp(&other.key)
        }
    }

    impl PartialOrd for CreateHeapItem {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut create_heap: BinaryHeap<Reverse<CreateHeapItem>> = BinaryHeap::new();
    for (idx, reader) in create_readers.iter().enumerate() {
        if let Some(key) = reader.key() {
            create_heap.push(Reverse(CreateHeapItem {
                key,
                reader_idx: idx,
            }));
        }
    }

    // Build initial heap for spends
    #[derive(Eq, PartialEq)]
    struct SpendHeapItem {
        key: ([u8; 32], u32),
        reader_idx: usize,
    }

    impl Ord for SpendHeapItem {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.key.cmp(&other.key)
        }
    }

    impl PartialOrd for SpendHeapItem {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    let mut spend_heap: BinaryHeap<Reverse<SpendHeapItem>> = BinaryHeap::new();
    for (idx, reader) in spend_readers.iter().enumerate() {
        if let Some(key) = reader.key() {
            spend_heap.push(Reverse(SpendHeapItem {
                key,
                reader_idx: idx,
            }));
        }
    }

    // Merge: output creates that are not spent
    while let Some(Reverse(create_item)) = create_heap.pop() {
        let create_key = create_item.key;

        // Skip all spends that are <= this create
        while let Some(Reverse(spend_item)) = spend_heap.peek() {
            if spend_item.key < create_key {
                // Advance spend reader
                let idx = spend_item.reader_idx;
                spend_heap.pop();
                spend_readers[idx].advance()?;
                if let Some(key) = spend_readers[idx].key() {
                    spend_heap.push(Reverse(SpendHeapItem {
                        key,
                        reader_idx: idx,
                    }));
                }
            } else {
                break;
            }
        }

        // Check if this create is spent
        let is_spent = spend_heap
            .peek()
            .map(|Reverse(item)| item.key == create_key)
            .unwrap_or(false);

        if !is_spent {
            // Output UTXO hash
            let entry = &create_readers[create_item.reader_idx]
                .current
                .as_ref()
                .unwrap();
            let utxo_hash = hash_utxo(&entry.txid, entry.vout, entry.value, &entry.script);
            output.write_all(&utxo_hash)?;
            utxo_count += 1;
        }

        // Advance create reader
        create_readers[create_item.reader_idx].advance()?;
        if let Some(key) = create_readers[create_item.reader_idx].key() {
            create_heap.push(Reverse(CreateHeapItem {
                key,
                reader_idx: create_item.reader_idx,
            }));
        }
    }

    output.flush()?;
    Ok(utxo_count)
}
