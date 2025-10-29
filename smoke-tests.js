#!/usr/bin/env node

const axios = require('axios');

const BASE_URL = process.env.STAGING_URL || 'http://localhost:8080';

async function runSmokeTests() {
  console.log('🚀 Starting smoke tests for JustIngredients...');
  console.log(`📍 Testing against: ${BASE_URL}`);

  const tests = [
    {
      name: 'Health Check - Live',
      url: `${BASE_URL}/health/live`,
      method: 'GET',
      expectStatus: 200
    },
    {
      name: 'Health Check - Ready',
      url: `${BASE_URL}/health/ready`,
      method: 'GET',
      expectStatus: 200
    },
    {
      name: 'Bot Webhook Endpoint',
      url: `${BASE_URL}/webhook`,
      method: 'POST',
      data: { message: 'smoke test', chat_id: '12345' },
      expectStatus: 200
    }
  ];

  let passed = 0;
  let failed = 0;

  for (const test of tests) {
    try {
      console.log(`\n🧪 Running: ${test.name}`);

      const response = await axios({
        method: test.method,
        url: test.url,
        data: test.data,
        timeout: 10000,
        validateStatus: () => true // Don't throw on non-2xx
      });

      if (response.status === test.expectStatus) {
        console.log(`✅ PASSED - Status: ${response.status}`);
        passed++;
      } else {
        console.log(`❌ FAILED - Expected: ${test.expectStatus}, Got: ${response.status}`);
        console.log(`   Response: ${JSON.stringify(response.data)}`);
        failed++;
      }
    } catch (error) {
      console.log(`❌ FAILED - Error: ${error.message}`);
      failed++;
    }
  }

  console.log('\n📊 Smoke Test Results:');
  console.log(`✅ Passed: ${passed}`);
  console.log(`❌ Failed: ${failed}`);
  console.log(`📈 Success Rate: ${((passed / (passed + failed)) * 100).toFixed(1)}%`);

  if (failed > 0) {
    console.log('\n💥 Smoke tests failed! Deployment should not proceed.');
    process.exit(1);
  } else {
    console.log('\n🎉 All smoke tests passed! Ready for production deployment.');
  }
}

runSmokeTests().catch(error => {
  console.error('💥 Smoke test runner failed:', error);
  process.exit(1);
});