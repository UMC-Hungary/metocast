import Tauri
import UIKit
import WebKit

struct NavItem: Decodable {
  let label: String
  let symbol: String
}

struct InstallArgs: Decodable {
  let items: [NavItem]
  let active: Int
  let onSelect: Channel
}

struct ActiveArgs: Decodable {
  let active: Int
}

class NativeNavPlugin: Plugin {
  private let barHeight: CGFloat = 62
  private let selectionInset: CGFloat = 4
  private var bar: UIVisualEffectView?
  private var selection: UIVisualEffectView?
  private var selectionEdges: [NSLayoutConstraint] = []
  private var symbols: [String] = []
  private var buttons: [UIButton] = []
  private var channel: Channel?
  private var active = 0

  @objc public func install(_ invoke: Invoke) throws {
    guard #available(iOS 15.0, *) else {
      invoke.reject("native navigation needs iOS 15 or later")
      return
    }
    let args = try invoke.parseArgs(InstallArgs.self)
    channel = args.onSelect
    active = args.active
    DispatchQueue.main.async { self.build(items: args.items) }
    invoke.resolve()
  }

  @objc public func setActive(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ActiveArgs.self)
    active = args.active
    if #available(iOS 15.0, *) {
      DispatchQueue.main.async { self.paint(animated: true) }
    }
    invoke.resolve()
  }

  @available(iOS 15.0, *)
  private func build(items: [NavItem]) {
    guard let host = manager.viewController?.view else { return }

    bar?.removeFromSuperview()
    symbols = items.map { $0.symbol }
    buttons = items.enumerated().map { index, item in
      var config = UIButton.Configuration.plain()
      config.title = item.label
      config.imagePlacement = .top
      config.imagePadding = 2
      config.contentInsets = NSDirectionalEdgeInsets(top: 5, leading: 6, bottom: 5, trailing: 6)
      config.titleTextAttributesTransformer = UIConfigurationTextAttributesTransformer { incoming in
        var out = incoming
        out.font = UIFont.systemFont(ofSize: 11, weight: .semibold)
        return out
      }
      let button = UIButton(configuration: config)
      button.tag = index
      button.addTarget(self, action: #selector(tapped(_:)), for: .touchUpInside)
      return button
    }

    let effect: UIVisualEffect
    if #available(iOS 26.0, *) {
      let glass = UIGlassEffect()
      glass.isInteractive = true
      effect = glass
    } else {
      effect = UIBlurEffect(style: .systemUltraThinMaterial)
    }

    let bar = UIVisualEffectView(effect: effect)
    bar.translatesAutoresizingMaskIntoConstraints = false
    bar.clipsToBounds = true
    self.bar = bar

    // The selection rides behind the items so it can slide between them. On iOS 26 it is
    // its own pane of glass, which is what makes it read as a lozenge rather than a wash.
    let selection: UIVisualEffectView
    if #available(iOS 26.0, *) {
      let glass = UIGlassEffect()
      glass.tintColor = UIColor.systemBackground.withAlphaComponent(0.85)
      selection = UIVisualEffectView(effect: glass)
    } else {
      selection = UIVisualEffectView(effect: nil)
      selection.backgroundColor = UIColor.label.withAlphaComponent(0.09)
    }
    selection.translatesAutoresizingMaskIntoConstraints = false
    selection.clipsToBounds = true
    selection.isUserInteractionEnabled = false
    // Glass on glass is invisible over a flat backdrop; the hairline keeps the lozenge legible.
    selection.layer.borderWidth = 0.5
    selection.layer.borderColor = UIColor.label.withAlphaComponent(0.1).cgColor
    self.selection = selection
    bar.contentView.addSubview(selection)

    let stack = UIStackView(arrangedSubviews: buttons)
    stack.axis = .horizontal
    stack.distribution = .fillEqually
    stack.translatesAutoresizingMaskIntoConstraints = false
    bar.contentView.addSubview(stack)

    host.addSubview(bar)
    let guide = host.safeAreaLayoutGuide
    NSLayoutConstraint.activate([
      bar.leadingAnchor.constraint(equalTo: guide.leadingAnchor, constant: 12),
      bar.trailingAnchor.constraint(equalTo: guide.trailingAnchor, constant: -12),
      bar.bottomAnchor.constraint(equalTo: guide.bottomAnchor, constant: -10),
      bar.heightAnchor.constraint(equalToConstant: barHeight),
      stack.topAnchor.constraint(equalTo: bar.contentView.topAnchor),
      stack.bottomAnchor.constraint(equalTo: bar.contentView.bottomAnchor),
      stack.leadingAnchor.constraint(equalTo: bar.contentView.leadingAnchor, constant: 4),
      stack.trailingAnchor.constraint(equalTo: bar.contentView.trailingAnchor, constant: -4),
      selection.topAnchor.constraint(equalTo: bar.contentView.topAnchor, constant: selectionInset),
      selection.bottomAnchor.constraint(equalTo: bar.contentView.bottomAnchor, constant: -selectionInset),
    ])
    if #available(iOS 26.0, *) {
      bar.cornerConfiguration = .capsule()
      selection.cornerConfiguration = .capsule()
    } else {
      bar.layer.cornerRadius = barHeight / 2
      selection.layer.cornerRadius = (barHeight - selectionInset * 2) / 2
    }

    paint(animated: false)
  }

  @available(iOS 15.0, *)
  private func paint(animated: Bool) {
    for (index, button) in buttons.enumerated() {
      let selected = index == active
      let weight: UIImage.SymbolWeight = selected ? .semibold : .regular
      let name = symbols[index]
      let image =
        (selected ? UIImage(systemName: name + ".fill") : nil) ?? UIImage(systemName: name)
      var config = button.configuration
      config?.image = image?.withConfiguration(
        UIImage.SymbolConfiguration(pointSize: 20, weight: weight))
      config?.baseForegroundColor = .label
      button.configuration = config
    }

    guard let bar, let selection, buttons.indices.contains(active) else { return }
    let target = buttons[active]
    bar.contentView.layoutIfNeeded()
    // Hug the item's own content rather than its stretched cell, so the pill stays a
    // lozenge instead of filling a quarter of the bar.
    let width = min(target.intrinsicContentSize.width + 20, target.bounds.width)
    selectionEdges.forEach { $0.isActive = false }
    selectionEdges = [
      selection.centerXAnchor.constraint(equalTo: target.centerXAnchor),
      selection.widthAnchor.constraint(equalToConstant: max(width, barHeight - selectionInset * 2)),
    ]
    selectionEdges.forEach { $0.isActive = true }

    guard animated else {
      bar.contentView.layoutIfNeeded()
      return
    }
    UIView.animate(withDuration: 0.38, delay: 0, usingSpringWithDamping: 0.78, initialSpringVelocity: 0.4) {
      bar.contentView.layoutIfNeeded()
    }
  }

  @available(iOS 15.0, *)
  @objc private func tapped(_ sender: UIButton) {
    active = sender.tag
    paint(animated: true)
    try? channel?.send(sender.tag)
  }
}

@_cdecl("init_plugin_native_nav")
func initPlugin() -> Plugin {
  return NativeNavPlugin()
}
