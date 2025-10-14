#!/bin/bash
# Test script for JustIngredients observability features

echo "🧪 Testing JustIngredients Observability"
echo "========================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to check if command succeeded
check_status() {
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ $1${NC}"
    else
        echo -e "${RED}❌ $1${NC}"
    fi
}

echo "1. Starting application with observability..."
cd /Users/basile.du.plessis/Documents/JustIngredients

# Start the app in background
cargo run > app.log 2>&1 &
APP_PID=$!

echo "   App started with PID: $APP_PID"
echo "   Waiting 5 seconds for initialization..."
sleep 5

echo ""
echo "2. Testing health endpoints..."

# Test liveness
echo "   Testing liveness probe..."
LIVE_RESPONSE=$(curl -s -w "%{http_code}" http://localhost:8080/health/live)
LIVE_CODE=$(echo $LIVE_RESPONSE | grep -o '[0-9]\+$')
LIVE_BODY=$(echo $LIVE_RESPONSE | sed 's/[0-9]\+$//')

if [ "$LIVE_CODE" = "200" ] && [ "$LIVE_BODY" = "OK" ]; then
    echo -e "   ${GREEN}✅ Liveness: $LIVE_BODY (HTTP $LIVE_CODE)${NC}"
else
    echo -e "   ${RED}❌ Liveness: $LIVE_BODY (HTTP $LIVE_CODE)${NC}"
fi

# Test readiness
echo "   Testing readiness probe..."
READY_RESPONSE=$(curl -s -w "%{http_code}" http://localhost:8080/health/ready)
READY_CODE=$(echo $READY_RESPONSE | grep -o '[0-9]\+$')
READY_BODY=$(echo $READY_RESPONSE | sed 's/[0-9]\+$//')

if [ "$READY_CODE" = "200" ] && [ "$READY_BODY" = "OK" ]; then
    echo -e "   ${GREEN}✅ Readiness: $READY_BODY (HTTP $READY_CODE)${NC}"
else
    echo -e "   ${YELLOW}⚠️  Readiness: $READY_BODY (HTTP $READY_CODE) - May be expected if DB/bot not configured${NC}"
fi

echo ""
echo "3. Testing metrics endpoint..."

# Test metrics
METRICS_RESPONSE=$(curl -s -w "%{http_code}" http://localhost:8080/metrics)
METRICS_CODE=$(echo $METRICS_RESPONSE | grep -o '[0-9]\+$')
METRICS_BODY=$(echo $METRICS_RESPONSE | sed 's/[0-9]\+$//')

if [ "$METRICS_CODE" = "200" ]; then
    echo -e "   ${GREEN}✅ Metrics endpoint responding (HTTP $METRICS_CODE)${NC}"

    # Check for key metrics
    if echo "$METRICS_BODY" | grep -q "ocr_operations_total"; then
        echo -e "   ${GREEN}✅ OCR metrics found${NC}"
    else
        echo -e "   ${YELLOW}⚠️  OCR metrics not found (expected if no OCR operations)${NC}"
    fi

    if echo "$METRICS_BODY" | grep -q "db_operations_total"; then
        echo -e "   ${GREEN}✅ Database metrics found${NC}"
    else
        echo -e "   ${YELLOW}⚠️  Database metrics not found (expected if no DB operations)${NC}"
    fi

    if echo "$METRICS_BODY" | grep -q "requests_total"; then
        echo -e "   ${GREEN}✅ Request metrics found${NC}"
    else
        echo -e "   ${YELLOW}⚠️  Request metrics not found (expected if no requests)${NC}"
    fi
else
    echo -e "   ${RED}❌ Metrics endpoint failed (HTTP $METRICS_CODE)${NC}"
fi

echo ""
echo "4. Checking application logs..."
if [ -f "app.log" ]; then
    echo "   Recent log entries:"
    tail -10 app.log | while read line; do
        echo "   $line"
    done
else
    echo -e "   ${YELLOW}⚠️  No app.log file found${NC}"
fi

echo ""
echo "5. Cleanup..."
echo "   Stopping application (PID: $APP_PID)..."
kill $APP_PID 2>/dev/null
sleep 2

if ps -p $APP_PID > /dev/null 2>&1; then
    echo -e "   ${YELLOW}⚠️  App still running, force killing...${NC}"
    kill -9 $APP_PID 2>/dev/null
fi

echo -e "${GREEN}✅ Test completed!${NC}"
echo ""
echo "📊 To monitor metrics continuously:"
echo "   curl -s http://localhost:8080/metrics | grep -E '(ocr_operations_total|db_operations_total|requests_total)'"
echo ""
echo "🔍 To view structured logs:"
echo "   tail -f app.log | jq . 2>/dev/null || tail -f app.log"