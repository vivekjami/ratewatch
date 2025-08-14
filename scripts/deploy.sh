#!/bin/bash
# Deployment script for RateWatch

set -euo pipefail

ENVIRONMENT="${1:-staging}"
NAMESPACE="ratewatch"

echo "🚀 Deploying RateWatch to $ENVIRONMENT"
echo "======================================"

# Validate environment
if [[ "$ENVIRONMENT" != "staging" && "$ENVIRONMENT" != "production" ]]; then
    echo "❌ Invalid environment. Use 'staging' or 'production'"
    exit 1
fi

# Check if kubectl is available
if ! command -v kubectl &> /dev/null; then
    echo "❌ kubectl is not installed or not in PATH"
    exit 1
fi

# Check if we can connect to the cluster
if ! kubectl cluster-info &> /dev/null; then
    echo "❌ Cannot connect to Kubernetes cluster"
    exit 1
fi

echo "✅ Connected to Kubernetes cluster"

# Create namespace if it doesn't exist
kubectl apply -f deploy/k8s/namespace.yaml

# Apply configurations
echo "📝 Applying configurations..."
kubectl apply -f deploy/k8s/configmap.yaml
kubectl apply -f deploy/k8s/secret.yaml

# Deploy Redis
echo "🔴 Deploying Redis..."
kubectl apply -f deploy/k8s/redis.yaml

# Wait for Redis to be ready
echo "⏳ Waiting for Redis to be ready..."
kubectl wait --for=condition=available --timeout=300s deployment/ratewatch-redis -n $NAMESPACE

# Deploy RateWatch
echo "🎯 Deploying RateWatch..."
kubectl apply -f deploy/k8s/deployment.yaml
kubectl apply -f deploy/k8s/service.yaml

# Wait for deployment to be ready
echo "⏳ Waiting for RateWatch to be ready..."
kubectl wait --for=condition=available --timeout=300s deployment/ratewatch -n $NAMESPACE

# Apply ingress for production
if [[ "$ENVIRONMENT" == "production" ]]; then
    echo "🌐 Applying ingress..."
    kubectl apply -f deploy/k8s/ingress.yaml
fi

# Apply monitoring
if kubectl get crd servicemonitors.monitoring.coreos.com &> /dev/null; then
    echo "📊 Applying monitoring..."
    kubectl apply -f deploy/k8s/servicemonitor.yaml
fi

# Get service information
echo ""
echo "📋 Deployment Information"
echo "========================"
kubectl get pods -n $NAMESPACE
echo ""
kubectl get services -n $NAMESPACE

# Health check
echo ""
echo "🏥 Running health check..."
if kubectl get service ratewatch-service -n $NAMESPACE &> /dev/null; then
    # Port forward for health check
    kubectl port-forward service/ratewatch-service 8081:80 -n $NAMESPACE &
    PF_PID=$!
    sleep 5
    
    if curl -s http://localhost:8081/health > /dev/null; then
        echo "✅ Health check passed!"
    else
        echo "❌ Health check failed!"
    fi
    
    kill $PF_PID 2>/dev/null || true
fi

echo ""
echo "🎉 Deployment to $ENVIRONMENT completed successfully!"