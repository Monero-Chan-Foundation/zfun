import { ReactNode } from "react"

interface InfoCardProps {
  title?: string | ReactNode
  description?: string | ReactNode
  children: ReactNode
  className?: string
  headerContent?: ReactNode
}

export function InfoCard({ 
  title, 
  description, 
  children, 
  className = "",
  headerContent
}: InfoCardProps) {
  return (
    <div className={`border border-white/10 bg-white/[0.02] backdrop-blur-sm overflow-hidden ${className}`}>
      <div className="flex flex-col">
        {/* Header Section */}
        <div className="p-4">
          {headerContent || (
            <>
              {typeof title === "string" ? (
                <h3 className="text-xs text-white/80 uppercase tracking-wider">{title}</h3>
              ) : (
                title
              )}
              {description && (
                typeof description === "string" ? (
                  <p className="text-xs text-white/40 mt-2">{description}</p>
                ) : (
                  description
                )
              )}
            </>
          )}
        </div>

        {/* Divider */}
        <div className="h-px bg-gradient-to-r from-white/10 to-transparent"></div>

        {/* Content Section */}
        <div className="p-4">
          {children}
        </div>
      </div>
    </div>
  )
}

