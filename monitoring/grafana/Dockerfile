FROM grafana/grafana:latest

# Set environment variables for Grafana
ENV GF_SECURITY_ADMIN_PASSWORD=admin
ENV GF_USERS_ALLOW_SIGN_UP=false
ENV GF_INSTALL_PLUGINS=

# Copy provisioning configuration
COPY monitoring/grafana/provisioning /etc/grafana/provisioning
COPY monitoring/grafana/dashboards /etc/grafana/dashboards
COPY monitoring/grafana/grafana.ini /etc/grafana/grafana.ini

EXPOSE 3000

CMD ["grafana-server", "--homepath=/usr/share/grafana", "--config=/etc/grafana/grafana.ini"]