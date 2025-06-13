use ocl::{Platform, Device, Context, Queue, Program, Buffer, MemFlags, Kernel};
use ocl::core::{DeviceInfo, PlatformInfo};
use colored::*;
use secp256k1::{PublicKey, SecretKey, Secp256k1};
use secp256k1::hashes::{sha256, Hash};
use hex::encode;
use num_bigint::BigUint;

const GPU_BATCH_SIZE: usize = 1024;

// Simplified OpenCL kernel for CLC mining
const CLC_MINING_KERNEL: &str = r#"
__kernel void clc_mine(
    __global uint* nonces,
    __global uchar* seed_data,
    uint seed_length,
    __global uchar* target_bytes,
    __global uint* result_found,
    __global uint* result_nonce
) {
    uint gid = get_global_id(0);
    uint base_nonce = nonces[0];
    uint current_nonce = base_nonce + gid;
    
    // Simple hash simulation - in real implementation this would be full secp256k1 + SHA256
    uint hash_value = current_nonce;
    for (uint i = 0; i < seed_length && i < 32; i++) {
        hash_value = hash_value * 1103515245 + seed_data[i] + 12345;
    }
    
    // Compare with target (simplified comparison)
    uint target_value = 0;
    for (int i = 0; i < 4; i++) {
        target_value = (target_value << 8) | target_bytes[i];
    }
    
    // Check if we found a solution (very simplified check)
    if (hash_value < target_value) {
        result_found[0] = 1;
        result_nonce[0] = current_nonce;
    }
    
    nonces[gid] = hash_value;
}
"#;

pub struct GPUMiner {
    platform: Platform,
    device: Device,
    context: Context,
    queue: Queue,
    program: Program,
    kernel: Kernel,
    
    // Simplified buffers
    nonces_buf: Buffer<u32>,
    seed_buf: Buffer<u8>,
    target_buf: Buffer<u8>,
    result_found_buf: Buffer<u32>,
    result_nonce_buf: Buffer<u32>,
    
    batch_size: usize,
}

impl GPUMiner {
    pub async fn new(device_index: usize) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Get OpenCL platform
        let platform = Platform::default();
        
        // Get devices
        let devices = Device::list_all(&platform)?;
        if device_index >= devices.len() {
            return Err(format!("Device index {} not available", device_index).into());
        }
        
        let device = devices[device_index];
        
        println!("{} Using GPU: {}", "[GPU]".green(), Self::get_device_name(&device));
        println!("{} Global Memory: {} MB", "[GPU]".green(), Self::get_device_memory(&device) / 1024 / 1024);
        println!("{} Compute Units: {}", "[GPU]".green(), Self::get_compute_units(&device));
        
        // Create context and queue
        let context = Context::builder()
            .platform(platform)
            .devices(&device)
            .build()?;
            
        let queue = Queue::new(&context, device, None)?;
        
        // Build program
        let program = Program::builder()
            .devices(&device)
            .src(CLC_MINING_KERNEL)
            .build(&context)?;
            
        // Create buffers first
        let nonces_buf = Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(MemFlags::READ_WRITE)
            .len(GPU_BATCH_SIZE)
            .build()?;
            
        let seed_buf = Buffer::<u8>::builder()
            .queue(queue.clone())
            .flags(MemFlags::READ_ONLY)
            .len(64) // Maximum seed length
            .build()?;
            
        let target_buf = Buffer::<u8>::builder()
            .queue(queue.clone())
            .flags(MemFlags::READ_ONLY)
            .len(32) // 256-bit target
            .build()?;
            
        let result_found_buf = Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(MemFlags::WRITE_ONLY)
            .len(1)
            .build()?;
            
        let result_nonce_buf = Buffer::<u32>::builder()
            .queue(queue.clone())
            .flags(MemFlags::WRITE_ONLY)
            .len(1)
            .build()?;
            
        // Create kernel with proper argument initialization
        let kernel = Kernel::builder()
            .program(&program)
            .name("clc_mine")
            .queue(queue.clone())
            .global_work_size(GPU_BATCH_SIZE)
            .arg(&nonces_buf)
            .arg(&seed_buf)
            .arg(0u32) // seed_length placeholder
            .arg(&target_buf)
            .arg(&result_found_buf)
            .arg(&result_nonce_buf)
            .build()?;
        
        Ok(GPUMiner {
            platform,
            device,
            context,
            queue,
            program,
            kernel,
            nonces_buf,
            seed_buf,
            target_buf,
            result_found_buf,
            result_nonce_buf,
            batch_size: GPU_BATCH_SIZE,
        })
    }
    
    pub async fn mine_batch(
        &mut self,
        target_diff: &BigUint,
        seed: &str,
        base_nonce: u64
    ) -> Result<Option<(SecretKey, PublicKey, String)>, Box<dyn std::error::Error + Send + Sync>> {
        // Prepare seed data
        let seed_bytes = seed.as_bytes();
        let seed_len = seed_bytes.len().min(64);
        
        // Prepare target data (simplified - just use first 32 bytes)
        let target_bytes = target_diff.to_bytes_be();
        let mut target_array = vec![0u8; 32];
        let copy_len = target_bytes.len().min(32);
        target_array[32 - copy_len..].copy_from_slice(&target_bytes[..copy_len]);
        
        // Prepare nonce data
        let nonces = vec![base_nonce as u32; self.batch_size];
        
        // Write data to GPU buffers
        self.nonces_buf.write(&nonces).enq()?;
        self.seed_buf.write(&seed_bytes[..seed_len]).enq()?;
        self.target_buf.write(&target_array).enq()?;
        
        // Initialize result buffers
        let zero_buf = vec![0u32; 1];
        self.result_found_buf.write(&zero_buf).enq()?;
        self.result_nonce_buf.write(&zero_buf).enq()?;
        
        // Update kernel arguments by index (arguments already set during kernel creation)
        self.kernel.set_arg(0, &self.nonces_buf)?;
        self.kernel.set_arg(1, &self.seed_buf)?;
        self.kernel.set_arg(2, seed_len as u32)?;
        self.kernel.set_arg(3, &self.target_buf)?;
        self.kernel.set_arg(4, &self.result_found_buf)?;
        self.kernel.set_arg(5, &self.result_nonce_buf)?;
        
        // Execute kernel
        unsafe {
            self.kernel.enq()?;
        }
        
        // Read results
        let mut found = vec![0u32; 1];
        let mut result_nonce = vec![0u32; 1];
        
        self.result_found_buf.read(&mut found).enq()?;
        self.result_nonce_buf.read(&mut result_nonce).enq()?;
        
        // Wait for completion
        self.queue.finish()?;
        
        // Check if solution was found
        if found[0] == 1 {
            // Generate actual key pair using CPU for the winning nonce
            let secp = Secp256k1::new();
            let mut rng = secp256k1::rand::rngs::OsRng;
            
            // In real implementation, derive key from nonce
            let (secret_key, public_key) = secp.generate_keypair(&mut rng);
            
            // Hash the public key with seed
            let pub_key_hex = encode(public_key.serialize_uncompressed());
            let combined = format!("{}{}", pub_key_hex, seed);
            let hashed = sha256::Hash::hash(combined.as_bytes());
            
            return Ok(Some((secret_key, public_key, hashed.to_string())));
        }
        
        Ok(None)
    }
    
    fn get_device_name(device: &Device) -> String {
        match device.info(DeviceInfo::Name) {
            Ok(ocl::core::DeviceInfoResult::Name(name)) => name,
            _ => "Unknown GPU".to_string()
        }
    }
    
    fn get_device_memory(device: &Device) -> u64 {
        match device.info(DeviceInfo::GlobalMemSize) {
            Ok(ocl::core::DeviceInfoResult::GlobalMemSize(size)) => size,
            _ => 0
        }
    }
    
    fn get_compute_units(device: &Device) -> u32 {
        match device.info(DeviceInfo::MaxComputeUnits) {
            Ok(ocl::core::DeviceInfoResult::MaxComputeUnits(units)) => units,
            _ => 0
        }
    }
    
    pub fn get_device_info(&self) -> String {
        let name = Self::get_device_name(&self.device);
        let memory = Self::get_device_memory(&self.device) / 1024 / 1024;
        let compute_units = Self::get_compute_units(&self.device);
        
        format!("GPU: {} | Compute Units: {} | Memory: {} MB", name, compute_units, memory)
    }
    
    pub fn get_platform_info(&self) -> String {
        match self.platform.info(PlatformInfo::Name) {
            Ok(ocl::core::PlatformInfoResult::Name(name)) => name,
            _ => "Unknown Platform".to_string()
        }
    }
    
    pub fn get_context(&self) -> &Context {
        &self.context
    }
    
    pub fn get_queue(&self) -> &Queue {
        &self.queue
    }
    
    pub fn get_program(&self) -> &Program {
        &self.program
    }
    
    pub fn reset_buffers(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Clear result buffers
        let zero_buf = vec![0u32; 1];
        self.result_found_buf.write(&zero_buf).enq()?;
        self.result_nonce_buf.write(&zero_buf).enq()?;
        self.get_queue().finish()?;
        Ok(())
    }
    
    pub fn validate_context(&self) -> bool {
        let devices = self.get_context().devices();
        devices.len() > 0
    }
    
    pub fn get_program_build_info(&self) -> String {
        let program_devices = match self.get_program().devices() {
            Ok(devices) => devices.len(),
            Err(_) => 0
        };
        format!("GPU Program for device: {} (Program devices: {})", self.get_device_info(), program_devices)
    }
}

pub struct GPUMiningPool {
    miners: Vec<GPUMiner>,
    active_miners: usize,
}

impl GPUMiningPool {
    pub async fn new(gpu_count: usize) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut miners = Vec::new();
        let mut active_miners = 0;
        
        for i in 0..gpu_count {
            match GPUMiner::new(i).await {
                Ok(miner) => {
                    println!("{} Initialized GPU {}: {}", "[GPU]".green(), i, miner.get_device_info());
                    println!("{} Platform: {}", "[GPU]".blue(), miner.get_platform_info());
                    miners.push(miner);
                    active_miners += 1;
                }
                Err(e) => {
                    println!("{} Failed to initialize GPU {}: {}", "[GPU]".red(), i, e);
                }
            }
        }
        
        if active_miners == 0 {
            return Err("No GPU miners could be initialized".into());
        }
        
        println!("{} Successfully initialized {} GPU miners", "[GPU]".green(), active_miners);
        
        Ok(GPUMiningPool {
            miners,
            active_miners,
        })
    }
    
    pub async fn mine_parallel(
        &mut self,
        target_diff: &BigUint,
        seed: &str,
        base_nonce: u64
    ) -> Result<Option<(SecretKey, PublicKey, String)>, Box<dyn std::error::Error + Send + Sync>> {
        // Only use active miners for processing
        for (i, miner) in self.miners.iter_mut().take(self.active_miners).enumerate() {
            let target_diff = target_diff.clone();
            let seed = seed.to_string();
            let nonce = base_nonce + (i as u64 * GPU_BATCH_SIZE as u64);
            
            // Validate context and reset buffers before mining
            if !miner.validate_context() {
                println!("{} GPU {} context is invalid, skipping", "[GPU]".yellow(), i);
                continue;
            }
            
            if let Err(e) = miner.reset_buffers() {
                println!("Warning: Failed to reset GPU buffers: {}", e);
                println!("Build info: {}", miner.get_program_build_info());
            }
            
            // Run mining batch
            if let Ok(Some(solution)) = miner.mine_batch(&target_diff, &seed, nonce).await {
                return Ok(Some(solution));
            }
        }
        
        Ok(None)
    }
    
    pub fn get_active_miners(&self) -> usize {
        self.active_miners
    }
    
    pub fn get_total_compute_units(&self) -> u32 {
        self.miners.iter()
            .map(|miner| GPUMiner::get_compute_units(&miner.device))
            .sum()
    }
}