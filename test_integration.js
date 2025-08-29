#!/usr/bin/env node

// Test script to verify Tauri integration
import { spawn } from 'child_process';

async function testTauriIntegration() {
  console.log('🧪 Testing Tauri Integration...\n');

  // Test 1: Check if Tauri app builds
  console.log('1️⃣ Testing Tauri build...');
  try {
    await runCommand('cd', ['src-tauri', '&&', 'cargo', 'check']);
    console.log('✅ Tauri build test passed');
  } catch (error) {
    console.log('❌ Tauri build test failed:', error.message);
  }

  // Test 2: Check if deprovisioning script is accessible
  console.log('\n2️⃣ Testing deprovisioning script accessibility...');
  try {
    const result = await runCommand('ls', ['-la', '/usr/local/bin/deprovision_client.sh']);
    console.log('✅ Script accessibility test passed');
    console.log('Script info:', result);
  } catch (error) {
    console.log('❌ Script accessibility test failed:', error.message);
  }

  // Test 3: Test deprovisioning script with sudo
  console.log('\n3️⃣ Testing deprovisioning script execution...');
  try {
    await runCommand('sudo', ['/usr/local/bin/deprovision_client.sh', '--help']);
    console.log('✅ Script execution test passed');
  } catch (error) {
    console.log('❌ Script execution test failed:', error.message);
  }

  // Test 4: Check sudoers configuration
  console.log('\n4️⃣ Testing sudoers configuration...');
  try {
    await runCommand('sudo', ['-u', 'dhcpd', 'sudo', '-n', '/usr/local/bin/deprovision_client.sh', '--help']);
    console.log('✅ Sudoers configuration test passed');
  } catch (error) {
    console.log('❌ Sudoers configuration test failed:', error.message);
  }

  console.log('\n🎉 Integration test completed!');
  console.log('\n📋 Next steps:');
  console.log('1. Run "npm run tauri dev" to start the application');
  console.log('2. Right-click on a client in the table');
  console.log('3. Select "Deprovision Client" from the context menu');
  console.log('4. Configure deprovisioning options in the modal');
  console.log('5. Click "Deprovision Client" to execute');
}

function runCommand(command, args = []) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      shell: true
    });

    let stdout = '';
    let stderr = '';

    child.stdout.on('data', (data) => {
      stdout += data.toString();
    });

    child.stderr.on('data', (data) => {
      stderr += data.toString();
    });

    child.on('close', (code) => {
      if (code === 0) {
        resolve(stdout.trim());
      } else {
        reject(new Error(`Command failed with code ${code}: ${stderr}`));
      }
    });

    child.on('error', (error) => {
      reject(error);
    });
  });
}

// Run the test
testTauriIntegration().catch(console.error);
