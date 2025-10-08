# https://github.com/bootc-dev/bootc/issues/1640
%if 0%{?fedora} || 0%{?rhel} >= 10 || 0%{?rust_minor} >= 89
    %global new_cargo_macros 1
%else
    %global new_cargo_macros 0
%endif


Name:           cla-rust
Version:        0.1.0
Release:        1%{?dist}
Summary:        OpenAI-compatible proxy for command-line assistant


# 
# (Apache-2.0 OR MIT) AND BSD-3-Clause
# Apache-2.0
# Apache-2.0 OR BSL-1.0
# Apache-2.0 OR MIT
# Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT
# BSD-2-Clause OR Apache-2.0 OR MIT
# BSD-3-Clause
# ISC
# MIT
# MIT AND BSD-3-Clause
# MIT OR Apache-2.0
# Unicode-3.0
# Unlicense OR MIT
License:        (Apache-2.0 OR MIT) AND BSD-3-Clause AND Apache-2.0 AND (Apache-2.0 OR BSL-1.0) AND (Apache-2.0 OR MIT) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND (BSD-2-Clause OR Apache-2.0 OR MIT) AND BSD-3-Clause AND ISC AND MIT AND (MIT AND BSD-3-Clause) AND (MIT OR Apache-2.0) AND Unicode-3.0 AND (Unlicense OR MIT)
URL:            https://github.com/r0x0d/cla-rust
Source0:        %{url}/releases/download/v%{version}/cla-rust-%{version}.tar.zstd
Source1:        %{url}/releases/download/v%{version}/cla-rust-%{version}-vendor.tar.zstd

%if 0%{?rhel}
BuildRequires: rust-toolset
%else
BuildRequires: cargo-rpm-macros >= 25
%endif
BuildRequires: systemd
BuildRequires: openssl-devel

%description
CLAD (Command-Line Assistant Daemon) is an OpenAI-compatible proxy server
that sits between Goose and the command-line-assistant backend. It translates
between Goose's OpenAI-compatible chat completion API and the command-line-assistant's
custom message/context API format.

The package includes:
- clad: The main proxy server daemon
- c: A convenient CLI wrapper for goose commands

%prep
%autosetup -a1 -n %{name}-%{version}

# Default -v vendor config doesn't support non-crates.io deps (i.e. git)
cp .cargo/vendor-config.toml .
%cargo_prep -N 
cat vendor-config.toml >> .cargo/config.toml
rm vendor-config.toml

%build
# Build the main bootc binary
%cargo_build

%cargo_vendor_manifest

# https://pagure.io/fedora-rust/rust-packaging/issue/33
sed -i -e '/https:\/\//d' cargo-vendor.txt
%cargo_license_summary
%{cargo_license} > LICENSE.dependencies

%install
# # Install binaries
install -D -m 0755 target/release/clad %{buildroot}%{_bindir}/clad
install -D -m 0755 target/release/c %{buildroot}%{_bindir}/c

# # Install example configuration file
# install -D -m 0644 crates/clad/config.toml.example %{buildroot}%{_datadir}/%{name}/config.toml

%files
%license LICENSE
%license LICENSE.dependencies
%license cargo-vendor.txt
%{_bindir}/clad
%{_bindir}/c
# %{_datadir}/%{name}/config.toml.example
# %dir %{_sysconfdir}/%{name}

%changelog
* Tue Oct 07 2025 Rodolfo Olivieri <rolivier@redhat.com> - 0.1.0-1
- Initial package release
- Includes clad proxy server and c CLI wrapper

