Name:           cla-rust
Version:        0.1.0
Release:        1%{?dist}
Summary:        OpenAI-compatible proxy for command-line assistant

License:        MIT OR Apache-2.0
URL:            https://github.com/rhel-lightspeed/cla-rust
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  rust-packaging
BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  openssl-devel
BuildRequires:  pkgconfig(openssl)

%description
CLAD (Command-Line Assistant Daemon) is an OpenAI-compatible proxy server
that sits between Goose and the command-line-assistant backend. It translates
between Goose's OpenAI-compatible chat completion API and the command-line-assistant's
custom message/context API format.

The package includes:
- clad: The main proxy server daemon
- c: A convenient CLI wrapper for goose commands

%prep
%autosetup -n %{name}-%{version}
%cargo_prep

%build
%cargo_build

%install
# Install binaries
install -D -m 0755 target/release/clad %{buildroot}%{_bindir}/clad
install -D -m 0755 target/release/c %{buildroot}%{_bindir}/c

# Install example configuration file
install -D -m 0644 crates/clad/config.toml.example %{buildroot}%{_datadir}/%{name}/config.toml

# Create directory for user configurations
install -d -m 0755 %{buildroot}%{_sysconfdir}/%{name}

%files
%{_bindir}/clad
%{_bindir}/c
%{_datadir}/%{name}/config.toml.example
%dir %{_sysconfdir}/%{name}

%changelog
* Tue Oct 07 2025 Rodolfo Olivieri <rolivier@redhat.com> - 0.1.0-1
- Initial package release
- Includes clad proxy server and c CLI wrapper

